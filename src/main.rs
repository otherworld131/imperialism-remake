#![deny(warnings, clippy::all)]

use std::io::{self, Write};
use std::path::PathBuf;

use ::infrastructure::persistence;
use domain::economy::buildings::{Building, BuildingType};
use domain::economy::civilians::{next_civilian_id, parse_civilian_type};
use domain::game_state::{GameState, new_game};
use domain::hex::HexCoord;
use domain::map::infrastructure;
use domain::map::{HexMap, UnitId};
use domain::military::ships::{Ship, ShipType};
use domain::military::units::{ArmyUnit, ArmyUnitType};
use domain::nation::Nation;
use domain::turn::{TurnReport, calculate_score, process_turn};
use domain::types::*;

use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique UnitIds when building units via CLI.
static NEXT_UNIT_ID: AtomicU32 = AtomicU32::new(2_000_000);

// ── Colored output helpers ────────────────────────────────────────

#[allow(dead_code)]
fn color_green(s: &str) -> String {
    format!("\x1b[92m{}\x1b[0m", s)
}

#[allow(dead_code)]
fn color_red(s: &str) -> String {
    format!("\x1b[91m{}\x1b[0m", s)
}

#[allow(dead_code)]
fn color_yellow(s: &str) -> String {
    format!("\x1b[93m{}\x1b[0m", s)
}

#[allow(dead_code)]
fn color_bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --scenario flag
    let scenario_flag = args.iter().position(|a| a == "--scenario");

    println!("╔══════════════════════════════════════════════╗");
    println!("║         IMPERIALISM REMAKE v0.1.0            ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    let mut game = if let Some(idx) = scenario_flag {
        let scenario_id = args.get(idx + 1).map(|s| s.as_str()).unwrap_or_else(|| {
            println!("  Usage: cargo run -- --scenario <id> [nation_index]");
            println!();
            println!("  Available scenarios:");
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
            std::process::exit(0);
        });
        let nation_index: usize = args.get(idx + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        match domain::scenarios::new_scenario_game(scenario_id, Difficulty::Normal, nation_index) {
            Ok(g) => {
                println!(
                    "  Starting scenario: {} ({})",
                    g.turn.year(),
                    domain::scenarios::list_scenarios()
                        .iter()
                        .find(|s| s.id == scenario_id)
                        .map(|s| s.name)
                        .unwrap_or("Unknown")
                );
                g
            }
            Err(e) => {
                println!("  Error: {}", e);
                println!();
                println!("  Available scenarios:");
                for s in domain::scenarios::list_scenarios() {
                    println!("    {} — {} ({})", s.id, s.name, s.year);
                }
                std::process::exit(1);
            }
        }
    } else {
        let map_key = args.get(1).map(|s| s.as_str()).unwrap_or("imperialism");
        let nation_index: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        new_game(map_key, Difficulty::Normal, nation_index)
    };

    // Show initial map
    println!("  Map key: \"{}\"", game.map_key);
    print_status(&game);
    println!();
    println!(
        "  MAP ({} x {}):",
        game.hex_map.width(),
        game.hex_map.height()
    );
    println!();
    render_map(&game.hex_map, &game.nations);
    println!();
    print_provinces(&game);
    println!();
    print_legend();
    println!();

    // Nation selection hints
    println!("  Tip: Circular nations allow faster railroad expansion.");
    println!("  Tip: Nations with 2+ Minor Nation neighbors enable easier trade.");
    println!();

    // ── Interactive game loop ────────────────────────────────────
    loop {
        print_prompt(&game);
        let input = read_line();
        let cmd = input.trim().to_lowercase();

        match cmd.as_str() {
            "q" | "quit" | "exit" => {
                println!("  Farewell, Your Excellency.");
                break;
            }
            "t" | "turn" | "end turn" | "" => {
                let report = process_turn(&mut game);
                print_turn_report(&game, &report);

                // Autosave (silent)
                let sdir = saves_dir();
                if !sdir.exists() {
                    std::fs::create_dir_all(&sdir).ok();
                }
                let autosave_path = sdir.join("autosave.json");
                persistence::save_game(&game, &autosave_path).ok();

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
                    print_game_end_summary(&game, &report);
                    break;
                }
            }
            "s" | "status" => {
                println!();
                print_status(&game);
            }
            "w" | "warehouse" => {
                println!();
                print_warehouse(&game);
            }
            "m" | "map" => {
                println!();
                render_map(&game.hex_map, &game.nations);
                println!();
                print_legend();
            }
            "p" | "provinces" => {
                println!();
                print_provinces(&game);
            }
            "n" | "nations" => {
                println!();
                print_nations(&game);
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
                print_buildings(&game);
            }
            "pop" | "population" => {
                println!();
                print_population(&game);
            }
            "tech" => {
                println!();
                print_tech(&game);
            }
            "score" => {
                println!();
                print_scores(&game);
            }
            "trade" => {
                println!();
                print_trade(&game);
            }
            "h" | "help" | "?" => {
                println!();
                print_help();
            }
            "overview" => {
                println!();
                print_overview(&game);
            }
            "history" => {
                println!();
                print_history(&game);
            }
            "orders" | "pending" => {
                println!();
                print_pending_orders(&game);
            }
            "turn10" => {
                cmd_auto(&mut game, 10);
            }
            "turn100" => {
                cmd_auto(&mut game, 100);
            }
            _ if cmd.starts_with("auto ") => {
                let count_str = input.trim()[5..].trim();
                match count_str.parse::<u32>() {
                    Ok(n) if n > 0 => {
                        cmd_auto(&mut game, n);
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
                save_current_game(&game);
            }
            "load" => {
                list_saved_games();
            }
            _ if cmd.starts_with("load ") => {
                let filename = input.trim()[5..].trim();
                match load_saved_game(filename) {
                    Ok(loaded) => {
                        game = loaded;
                        println!("  Game loaded successfully.");
                        print_status(&game);
                    }
                    Err(e) => {
                        println!("  Failed to load: {}", e);
                    }
                }
            }
            _ if cmd.starts_with("delete ") => {
                let filename = input.trim()[7..].trim();
                delete_saved_game(filename);
            }
            _ if cmd.starts_with("saveinfo ") => {
                let filename = input.trim()[9..].trim();
                cmd_saveinfo(filename);
            }
            "quicksave" | "qs" => {
                quicksave_game(&game);
            }
            "quickload" | "ql" => match load_saved_game("quicksave.json") {
                Ok(loaded) => {
                    game = loaded;
                    println!("  Quickload successful.");
                    print_status(&game);
                }
                Err(e) => {
                    println!("  Quickload failed: {}", e);
                }
            },
            _ if cmd.starts_with("research ") => {
                let tech_query = input.trim()[9..].trim();
                research_tech(&mut game, tech_query);
            }
            "build railroad" => {
                cmd_build_railroad(&mut game);
            }
            "build depot" => {
                cmd_build_depot(&mut game);
            }
            "build port" => {
                cmd_build_port(&mut game);
            }
            "build fort" => {
                cmd_build_fort(&mut game, None);
            }
            _ if cmd.starts_with("build fort ") => {
                let province_query = input.trim()[11..].trim();
                cmd_build_fort(&mut game, Some(province_query));
            }
            "infrastructure" | "infra" => {
                println!();
                print_infrastructure(&game);
            }
            "build militia" => {
                build_unit(&mut game, "militia");
            }
            "build freight" => {
                build_freight_car(&mut game);
            }
            _ if cmd.starts_with("build unit ") => {
                let unit_query = input.trim()[11..].trim();
                build_unit(&mut game, unit_query);
            }
            _ if cmd.starts_with("build ") => {
                let building_query = input.trim()[6..].trim();
                build_building(&mut game, building_query);
            }
            _ if cmd.starts_with("expand ") => {
                let building_query = input.trim()[7..].trim();
                expand_building(&mut game, building_query);
            }
            "recruit" => {
                recruit_worker(&mut game);
            }
            "train" => {
                train_worker(&mut game);
            }
            "diplomacy" | "diplo" | "d" => {
                println!();
                print_diplomacy(&game);
            }
            _ if cmd.starts_with("consulate ") => {
                let nation_query = input.trim()[10..].trim();
                cmd_consulate(&mut game, nation_query);
            }
            _ if cmd.starts_with("embassy ") => {
                let nation_query = input.trim()[8..].trim();
                cmd_embassy(&mut game, nation_query);
            }
            _ if cmd.starts_with("attack ") => {
                let nation_query = input.trim()[7..].trim();
                cmd_attack(&mut game, nation_query);
            }
            "transport" | "freight" => {
                println!();
                print_transport(&game);
            }
            "build car" => {
                build_freight_car(&mut game);
            }
            "military" | "army" => {
                println!();
                print_military(&game);
            }
            _ if cmd.starts_with("info ") => {
                let nation_query = input.trim()[5..].trim();
                println!();
                print_nation_info(&game, nation_query);
            }
            _ if cmd.starts_with("war ") => {
                let nation_query = input.trim()[4..].trim();
                cmd_war(&mut game, nation_query);
            }
            _ if cmd.starts_with("peace ") => {
                let nation_query = input.trim()[6..].trim();
                cmd_peace(&mut game, nation_query);
            }
            _ if cmd.starts_with("pact ") => {
                let nation_query = input.trim()[5..].trim();
                cmd_pact(&mut game, nation_query);
            }
            _ if cmd.starts_with("alliance ") => {
                let nation_query = input.trim()[9..].trim();
                cmd_alliance(&mut game, nation_query);
            }
            _ if cmd.starts_with("grant ") => {
                let args = input.trim()[6..].trim();
                cmd_grant(&mut game, args);
            }
            "civilians" => {
                println!();
                print_civilians(&game);
            }
            _ if cmd.starts_with("hire ") => {
                let type_query = input.trim()[5..].trim();
                cmd_hire_civilian(&mut game, type_query);
            }
            _ if cmd.starts_with("deploy ") => {
                let args = input.trim()[7..].trim();
                cmd_deploy_civilian(&mut game, args);
            }
            _ if cmd.starts_with("move ") => {
                let args = input.trim()[5..].trim();
                cmd_move_unit(&mut game, args);
            }
            "build ship trader" => {
                cmd_build_ship(&mut game, "trader");
            }
            "build ship indiaman" => {
                cmd_build_ship(&mut game, "indiaman");
            }
            _ if cmd.starts_with("build ship ") => {
                let ship_query = input.trim()[11..].trim();
                cmd_build_ship(&mut game, ship_query);
            }
            "fleet" => {
                println!();
                print_fleet(&game);
            }
            "navy" => {
                println!();
                print_navy(&game);
            }
            _ if cmd.starts_with("build warship ") => {
                let ship_query = input.trim()[14..].trim();
                cmd_build_warship(&mut game, ship_query);
            }
            "produce arms" => {
                cmd_produce_arms(&mut game);
            }
            _ if cmd.starts_with("blockade ") => {
                let nation_query = input.trim()[9..].trim();
                cmd_blockade(&game, nation_query);
            }
            _ if cmd.starts_with("sell ") => {
                let args = input.trim()[5..].trim();
                cmd_sell(&mut game, args);
            }
            _ if cmd.starts_with("upgrade ") => {
                let index_str = input.trim()[8..].trim();
                cmd_upgrade_unit(&mut game, index_str);
            }
            _ if cmd.starts_with("subsidy ") => {
                let args = input.trim()[8..].trim();
                cmd_subsidy(&mut game, args);
            }
            _ => {
                println!("  Unknown command. Type 'help' for available commands.");
            }
        }
    }
}

/// Check if the human player's nation is bankrupt. If so, print a message and return true.
fn check_bankrupt(game: &GameState) -> bool {
    let player = game.get_nation(game.human_player_nation).unwrap();
    if player.is_bankrupt() {
        println!(
            "  FINANCIAL CRISIS: Your nation is bankrupt (treasury: {}). No spending allowed until treasury recovers.",
            player.treasury
        );
        true
    } else {
        false
    }
}

/// Parse a building name string into a BuildingType.
/// Only mills and factories can be built by the player.
fn parse_buildable(name: &str) -> Option<BuildingType> {
    match name.to_lowercase().as_str() {
        "lumbermill" | "lumber mill" | "lumber_mill" => Some(BuildingType::LumberMill),
        "steelmill" | "steel mill" | "steel_mill" => Some(BuildingType::SteelMill),
        "textilemill" | "textile mill" | "textile_mill" => Some(BuildingType::TextileMill),
        "furniturefactory" | "furniture factory" | "furniture_factory" => {
            Some(BuildingType::FurnitureFactory)
        }
        "hardwarefactory" | "hardware factory" | "hardware_factory" => {
            Some(BuildingType::HardwareFactory)
        }
        "clothingfactory" | "clothing factory" | "clothing_factory" => {
            Some(BuildingType::ClothingFactory)
        }
        _ => None,
    }
}

fn build_building(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let bt = match parse_buildable(query) {
        Some(bt) => bt,
        None => {
            println!("  Unknown building: '{}'", query);
            println!(
                "  Available: lumbermill, steelmill, textilemill, furniturefactory, hardwarefactory, clothingfactory"
            );
            return;
        }
    };

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    // Check if the player already has this building
    if player.has_building(bt) {
        println!(
            "  You already have a {:?}. Use 'expand {:?}' to increase its capacity.",
            bt, bt
        );
        return;
    }

    // Cost: 1 lumber + 1 steel per capacity unit; initial capacity is 2
    let initial_capacity: u32 = 2;
    let (lumber_needed, steel_needed) = Building::expansion_cost(initial_capacity);

    let lumber_have = player.material_amount(MaterialType::Lumber);
    let steel_have = player.material_amount(MaterialType::Steel);

    if lumber_have < lumber_needed || steel_have < steel_needed {
        println!(
            "  Insufficient materials to build {:?} (need {} lumber, {} steel; have {} lumber, {} steel).",
            bt, lumber_needed, steel_needed, lumber_have, steel_have
        );
        return;
    }

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Steel, steel_needed);
    player.buildings.push(Building::new(bt, initial_capacity));

    println!(
        "  Built {:?} with capacity {} (consumed {} lumber, {} steel).",
        bt, initial_capacity, lumber_needed, steel_needed
    );
}

fn expand_building(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let bt = match parse_buildable(query) {
        Some(bt) => bt,
        None => {
            println!("  Unknown building: '{}'", query);
            println!(
                "  Available: lumbermill, steelmill, textilemill, furniturefactory, hardwarefactory, clothingfactory"
            );
            return;
        }
    };

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if !player.has_building(bt) {
        println!(
            "  You don't have a {:?} yet. Use 'build {:?}' first.",
            bt, bt
        );
        return;
    }

    // Calculate the expansion amount using capacity progression (2 -> 4 -> 8 -> 12 -> 16 -> ...)
    let current_capacity = player
        .buildings
        .iter()
        .find(|b| b.building_type == bt)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let next = match current_capacity {
        c if c < 2 => 2,
        2 => 4,
        4 => 8,
        8 => 12,
        12 => 16,
        16 => 20,
        _ => current_capacity + 4,
    };
    let expand_amount = next - current_capacity;

    // Cost: 1 lumber + 1 steel per new capacity unit
    let (lumber_needed, steel_needed) = Building::expansion_cost(expand_amount);

    let lumber_have = player.material_amount(MaterialType::Lumber);
    let steel_have = player.material_amount(MaterialType::Steel);

    if lumber_have < lumber_needed || steel_have < steel_needed {
        println!(
            "  Insufficient materials to expand {:?} (need {} lumber, {} steel; have {} lumber, {} steel).",
            bt, lumber_needed, steel_needed, lumber_have, steel_have
        );
        return;
    }

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Steel, steel_needed);

    let building = player.get_building_mut(bt).unwrap();
    building.start_expansion(expand_amount);

    println!(
        "  Expanding {:?} from {} to {} capacity (consumed {} lumber, {} steel). Will be ready in 2 turns.",
        bt, current_capacity, next, lumber_needed, steel_needed
    );
}

/// Calculate max recruitable workers based on province count.
/// Base: 1 worker per 4 provinces. With expanded Capitol: 1 per 3 provinces.
fn max_recruitment_capacity(player: &Nation) -> u32 {
    let province_count = player.province_count() as u32;
    let has_expanded_capitol = player
        .buildings
        .iter()
        .any(|b| b.building_type == BuildingType::Capitol && b.capacity > 1);
    let per_province = if has_expanded_capitol { 3 } else { 4 };
    if per_province == 0 {
        return 0;
    }
    province_count / per_province
}

fn recruit_worker(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    // Check province-based recruitment limit
    let max_recruits = max_recruitment_capacity(player);
    let current_workers = player.labor.total_workers();
    if current_workers >= max_recruits {
        println!(
            "  Cannot recruit: at capacity ({}/{} workers for {} provinces).",
            current_workers,
            max_recruits,
            player.province_count()
        );
        println!("  Conquer more provinces or expand your Capitol to increase capacity.");
        return;
    }

    let canned_food = player.material_amount(MaterialType::CannedFood);
    let clothing = player.goods_amount(GoodsType::Clothing);
    let furniture = player.goods_amount(GoodsType::Furniture);

    let mut missing = Vec::new();
    if canned_food < 1 {
        missing.push(format!("1 canned food (have {})", canned_food));
    }
    if clothing < 1 {
        missing.push(format!("1 clothing (have {})", clothing));
    }
    if furniture < 1 {
        missing.push(format!("1 furniture (have {})", furniture));
    }

    if !missing.is_empty() {
        println!("  Cannot recruit: missing {}", missing.join(", "));
        return;
    }

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::CannedFood, 1);
    player.consume_goods(GoodsType::Clothing, 1);
    player.consume_goods(GoodsType::Furniture, 1);
    player.labor.recruit_immigrant();

    let max_recruits = max_recruitment_capacity(player);
    println!(
        "  Recruited 1 untrained worker (now: {} untrained, {} trained, {} expert, capacity {}).",
        player.labor.untrained, player.labor.trained, player.labor.expert, max_recruits
    );
}

fn train_worker(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if player.labor.untrained == 0 {
        println!("  No untrained workers available to train.");
        return;
    }

    // Simplified: consume 1 paper if available, but allow training regardless
    let has_paper = player.material_amount(MaterialType::Paper) >= 1;

    let player = game.get_nation_mut(player_id).unwrap();
    if has_paper {
        player.consume_material(MaterialType::Paper, 1);
    }
    player.labor.train_worker();

    let paper_note = if has_paper {
        " (consumed 1 paper)"
    } else {
        " (no paper available, training proceeds anyway)"
    };
    println!(
        "  Trained 1 worker{} (now: {} untrained, {} trained, {} expert).",
        paper_note, player.labor.untrained, player.labor.trained, player.labor.expert
    );
}

/// Parse a unit type name string.
fn parse_unit_type(name: &str) -> Option<ArmyUnitType> {
    match name.to_lowercase().as_str() {
        "militia" => Some(ArmyUnitType::Militia),
        "regulars" => Some(ArmyUnitType::Regulars),
        "grenadiers" => Some(ArmyUnitType::Grenadiers),
        "cuirassiers" => Some(ArmyUnitType::Cuirassiers),
        "light artillery" | "lightartillery" | "light_artillery" => {
            Some(ArmyUnitType::LightArtillery)
        }
        _ => None,
    }
}

/// Simplified money cost for supported unit types.
fn unit_build_cost(unit_type: ArmyUnitType) -> Money {
    match unit_type {
        ArmyUnitType::Regulars => Money::dollars(500),
        ArmyUnitType::Grenadiers => Money::dollars(1000),
        ArmyUnitType::Cuirassiers => Money::dollars(500),
        ArmyUnitType::LightArtillery => Money::dollars(2000),
        _ => Money::dollars(0),
    }
}

fn build_unit(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let unit_type = match parse_unit_type(query) {
        Some(ut) => ut,
        None => {
            println!("  Unknown unit type: '{}'", query);
            println!(
                "  Available: regulars ($500), grenadiers ($1000), cuirassiers ($500), light artillery ($2000)"
            );
            return;
        }
    };

    let cost = unit_build_cost(unit_type);
    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if player.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford {:?} (cost: {}, treasury: {}).",
            unit_type, cost, player.treasury
        );
        return;
    }

    let capital_province = player.capital_province_id;
    let uid = UnitId(NEXT_UNIT_ID.fetch_add(1, Ordering::Relaxed));
    let unit = ArmyUnit::new(uid, unit_type, player_id, capital_province);

    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= cost;
    player.army.push(unit);

    println!(
        "  Unit built! {:?} stationed at capital (cost: {}, treasury now: {}). Army size: {}",
        unit_type,
        cost,
        player.treasury,
        player.army.len()
    );
}

/// Upgrade a unit at the given army index.
/// Costs $500. Preserves medals and health.
fn cmd_upgrade_unit(game: &mut GameState, index_str: &str) {
    if check_bankrupt(game) {
        return;
    }

    let idx: usize = match index_str.parse() {
        Ok(i) => i,
        Err(_) => {
            println!("  Usage: upgrade <unit_index> (e.g. upgrade 0)");
            return;
        }
    };

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if idx >= player.army.len() {
        println!(
            "  Invalid unit index {}. You have {} units (0-{}).",
            idx,
            player.army.len(),
            player.army.len().saturating_sub(1)
        );
        return;
    }

    let current_type = player.army[idx].unit_type;
    let target_type = match current_type.upgrade_to() {
        Some(t) => t,
        None => {
            println!("  {:?} has no available upgrade path.", current_type);
            return;
        }
    };

    // Check if prerequisite tech is researched
    if let Some(required_tech_name) = target_type.required_tech() {
        let has_tech = player.researched_techs.iter().any(|tid| {
            game.game_data
                .tech_tree
                .get(*tid)
                .map(|t| t.name == required_tech_name)
                .unwrap_or(false)
        });
        if !has_tech {
            println!(
                "  Cannot upgrade {:?} to {:?}: requires '{}' technology.",
                current_type, target_type, required_tech_name
            );
            return;
        }
    }

    // Check cost ($500 flat)
    let cost = Money::dollars(500);
    if player.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford upgrade (cost: {}, treasury: {}).",
            cost, player.treasury
        );
        return;
    }

    // Apply upgrade
    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= cost;
    let old_medals = player.army[idx].medals;
    let old_health = player.army[idx].health;
    player.army[idx].unit_type = target_type;
    player.army[idx].movement_remaining = target_type.stats().movement;
    // Preserve medals and health
    player.army[idx].medals = old_medals;
    player.army[idx].health = old_health;

    println!(
        "  Upgraded {:?} -> {:?} (medals: {}, health: {}, cost: {}, treasury: {}).",
        current_type, target_type, old_medals, old_health, cost, player.treasury
    );
}

/// Build a merchant ship (trader or indiaman).
fn cmd_build_ship(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let ship_type = match query.to_lowercase().as_str() {
        "trader" => ShipType::Trader,
        "indiaman" => ShipType::Indiaman,
        _ => {
            println!("  Unknown ship type: '{}'", query);
            println!(
                "  Available: trader (2 fabric + 4 lumber, cargo: 2), indiaman (3 fabric + 7 lumber, cargo: 4)"
            );
            return;
        }
    };

    let stats = ship_type.stats();
    let fabric_needed = stats.fabric_cost;
    let lumber_needed = stats.lumber_cost;

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    let fabric_have = player.material_amount(MaterialType::Fabric);
    let lumber_have = player.material_amount(MaterialType::Lumber);

    if fabric_have < fabric_needed || lumber_have < lumber_needed {
        println!(
            "  Insufficient materials to build {:?} (need {} fabric + {} lumber; have {} fabric + {} lumber).",
            ship_type, fabric_needed, lumber_needed, fabric_have, lumber_have
        );
        return;
    }

    let uid = UnitId(NEXT_UNIT_ID.fetch_add(1, Ordering::Relaxed));
    let ship = Ship::new(uid, ship_type, player_id);

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::Fabric, fabric_needed);
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.merchant_fleet.push(ship);

    println!(
        "  Ship built! {:?} added to merchant fleet (cargo: {}). Fleet size: {}, total cargo: {}",
        ship_type,
        stats.cargo,
        player.merchant_fleet.len(),
        player.total_cargo_capacity()
    );
}

/// Build a warship (frigate or ship-of-the-line).
fn cmd_build_warship(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let ship_type = match query.to_lowercase().as_str() {
        "frigate" => ShipType::Frigate,
        "ship-of-the-line" | "ship of the line" | "shipoftheline" => ShipType::ShipOfTheLine,
        _ => {
            println!("  Unknown warship type: '{}'", query);
            println!(
                "  Available: frigate (2 fabric + 5 lumber + 2 arms), ship-of-the-line (3 fabric + 8 lumber + 5 arms)"
            );
            return;
        }
    };

    let stats = ship_type.stats();
    let fabric_needed = stats.fabric_cost;
    let lumber_needed = stats.lumber_cost;
    let arms_needed = stats.arms_cost;

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    let fabric_have = player.material_amount(MaterialType::Fabric);
    let lumber_have = player.material_amount(MaterialType::Lumber);
    let arms_have = player.material_amount(MaterialType::Arms);

    if fabric_have < fabric_needed || lumber_have < lumber_needed || arms_have < arms_needed {
        println!(
            "  Insufficient materials to build {:?} (need {} fabric + {} lumber + {} arms; have {} fabric + {} lumber + {} arms).",
            ship_type,
            fabric_needed,
            lumber_needed,
            arms_needed,
            fabric_have,
            lumber_have,
            arms_have
        );
        return;
    }

    let uid = UnitId(NEXT_UNIT_ID.fetch_add(1, Ordering::Relaxed));
    let ship = Ship::new(uid, ship_type, player_id);

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::Fabric, fabric_needed);
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Arms, arms_needed);
    player.warships.push(ship);

    println!(
        "  Warship built! {:?} added to navy (FP: {}, Armor: {}, Hull: {}). Navy size: {}, total naval firepower: {}",
        ship_type,
        stats.firepower,
        stats.armor,
        stats.hull,
        player.warships.len(),
        player.total_naval_firepower()
    );
}

/// Print the warship fleet.
fn print_navy(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();

    if player.warships.is_empty() {
        println!(
            "  No warships. Use 'build warship frigate' or 'build warship ship-of-the-line' to build one."
        );
        return;
    }

    println!("  NAVY:");

    // Count ships by type
    let mut counts: std::collections::BTreeMap<String, (usize, u32, u32, u32)> =
        std::collections::BTreeMap::new();
    for ship in &player.warships {
        let name = format!("{:?}", ship.ship_type);
        let stats = ship.ship_type.stats();
        let entry = counts
            .entry(name)
            .or_insert((0, stats.firepower, stats.armor, stats.hull));
        entry.0 += 1;
    }

    for (name, (count, fp, armor, hull)) in &counts {
        println!(
            "    {}x {} (FP: {}, Armor: {}, Hull: {})",
            count, name, fp, armor, hull
        );
    }
    println!(
        "    Total naval firepower: {}",
        player.total_naval_firepower()
    );
}

/// Produce arms: convert 1 steel into 1 arms.
fn cmd_produce_arms(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    let steel_have = player.material_amount(MaterialType::Steel);
    if steel_have < 1 {
        println!("  Cannot produce arms: need 1 steel (have {}).", steel_have);
        return;
    }

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::Steel, 1);
    player.add_material(MaterialType::Arms, 1);

    println!(
        "  Produced 1 arms from 1 steel. Arms: {}, Steel: {}",
        player.material_amount(MaterialType::Arms),
        player.material_amount(MaterialType::Steel)
    );
}

/// Show blockade status against a target nation.
fn cmd_blockade(game: &GameState, query: &str) {
    let player_id = game.human_player_nation;
    let player = match game.get_nation(player_id) {
        Some(n) => n,
        None => return,
    };

    // Find the target nation by name
    let target = game
        .nations
        .iter()
        .find(|n| n.name.to_lowercase().contains(&query.to_lowercase()) && n.id != player_id);

    let target = match target {
        Some(t) => t,
        None => {
            println!(
                "  Unknown nation: '{}'. Use 'nations' to see all nations.",
                query
            );
            return;
        }
    };

    // Check if at war
    let at_war = game
        .diplomacy
        .get_relation(player_id, target.id)
        .is_some_and(|r| r.at_war);

    if !at_war {
        println!(
            "  You are not at war with {}. Blockade requires being at war.",
            target.name
        );
        return;
    }

    let our_warships = player.warship_count();
    if our_warships == 0 {
        println!("  You have no warships. Build warships first with 'build warship <type>'.");
        return;
    }

    let enemy_cargo = target.total_cargo_capacity();
    let blocked = domain::military::calculate_blockade_effect(enemy_cargo, our_warships as u32);
    let cargo_blocked = enemy_cargo.saturating_sub(blocked);

    println!("  NAVAL BLOCKADE vs {}:", target.name);
    println!("    Your warships: {}", our_warships);
    println!(
        "    Your naval firepower: {}",
        player.total_naval_firepower()
    );
    println!("    Enemy merchant cargo: {} holds", enemy_cargo);
    println!(
        "    Cargo blocked: {} (each warship blocks 2 holds)",
        cargo_blocked
    );
    println!("    Enemy effective cargo: {} holds", blocked);
    println!();
    println!("  Note: Blockade is applied automatically each turn while at war.");
    println!(
        "  Enemy warships ({}) will engage yours in naval combat.",
        target.warship_count()
    );
}

/// Print the merchant fleet.
fn print_fleet(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();

    if player.merchant_fleet.is_empty() {
        println!(
            "  No merchant ships. Use 'build ship trader' or 'build ship indiaman' to build one."
        );
        return;
    }

    println!("  MERCHANT FLEET:");

    // Count ships by type
    let mut counts: std::collections::BTreeMap<String, (usize, u32)> =
        std::collections::BTreeMap::new();
    for ship in &player.merchant_fleet {
        let name = format!("{:?}", ship.ship_type);
        let cargo = ship.total_cargo_capacity();
        let entry = counts.entry(name).or_insert((0, cargo));
        entry.0 += 1;
    }

    for (name, (count, cargo)) in &counts {
        println!("    {}x {} (cargo: {} each)", count, name, cargo);
    }
    println!(
        "    Total cargo capacity: {} holds",
        player.total_cargo_capacity()
    );
}

/// Sell resources at base price, adding revenue to treasury.
fn cmd_sell(game: &mut GameState, args: &str) {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() != 2 {
        println!("  Usage: sell <resource> <quantity>");
        println!("  Example: sell timber 5");
        return;
    }

    let resource = match parse_resource_type(parts[0]) {
        Some(r) => r,
        None => {
            println!("  Unknown resource: '{}'", parts[0]);
            println!(
                "  Tradeable: timber, coal, iron, cotton, wool, fruit, livestock, oil, gold, gems"
            );
            return;
        }
    };

    let quantity: u32 = match parts[1].parse() {
        Ok(q) if q > 0 => q,
        _ => {
            println!("  Quantity must be a positive number.");
            return;
        }
    };

    if !resource.is_tradeable() {
        println!("  {:?} is not tradeable.", resource);
        return;
    }

    let price = domain::economy::trade::base_price(resource);
    if price == Money::ZERO {
        println!("  {:?} has no trade value.", resource);
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if player.resource_amount(resource) < quantity {
        println!(
            "  Insufficient {:?} (have {}, want to sell {}).",
            resource,
            player.resource_amount(resource),
            quantity
        );
        return;
    }

    let revenue = price * quantity as i64;

    let player = game.get_nation_mut(player_id).unwrap();
    player.remove_resource(resource, quantity);
    player.treasury += revenue;

    println!(
        "  Sold {} {:?} at {} each for {} total. Treasury: {}",
        quantity, resource, price, revenue, player.treasury
    );
}

/// Parse a resource type from a string.
fn parse_resource_type(s: &str) -> Option<ResourceType> {
    match s.to_lowercase().as_str() {
        "timber" => Some(ResourceType::Timber),
        "coal" => Some(ResourceType::Coal),
        "iron" => Some(ResourceType::Iron),
        "cotton" => Some(ResourceType::Cotton),
        "wool" => Some(ResourceType::Wool),
        "fruit" => Some(ResourceType::Fruit),
        "livestock" => Some(ResourceType::Livestock),
        "oil" => Some(ResourceType::Oil),
        "gold" => Some(ResourceType::Gold),
        "gems" => Some(ResourceType::Gems),
        "grain" => Some(ResourceType::Grain),
        "horses" => Some(ResourceType::Horses),
        _ => None,
    }
}

/// Return the player's rank (1-based) and total score from the turn report scores.
#[allow(dead_code)]
fn score_summary(scores: &[(NationId, String, u32)], player_id: NationId) -> Option<(usize, u32)> {
    // Scores are already sorted descending by total.
    for (i, (nid, _, total)) in scores.iter().enumerate() {
        if *nid == player_id {
            return Some((i + 1, *total));
        }
    }
    None
}

/// Format a number with comma separators (e.g. 1290 -> "1,290").
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn print_game_end_summary(game: &GameState, report: &TurnReport) {
    println!("  ╔════════════════════════════════════════╗");
    println!("  ║        FINAL GAME SUMMARY              ║");
    println!("  ╚════════════════════════════════════════╝");
    println!();

    // Final scores for all Great Powers
    println!("    {:<12} {:>8} {:>8}", "Nation", "Score", "Provinces");
    println!("    {}", "-".repeat(32));

    let mut scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| {
            let s = calculate_score(n);
            (n.id, n.name.clone(), s.total, n.province_count())
        })
        .collect();
    scores.sort_by(|a, b| b.2.cmp(&a.2));

    for (id, name, total, prov_count) in &scores {
        let marker = if *id == game.human_player_nation {
            " <-- YOU"
        } else {
            ""
        };
        println!(
            "    {:<12} {:>8} {:>8}{}",
            name,
            format_number(*total),
            prov_count,
            marker
        );
    }
    println!();

    // Show Council of Governors vote details
    if let Some(ref vote) = report.council_vote {
        println!("  Council of Governors Vote:");
        println!("    {}", "-".repeat(40));

        // Show MN governor preferences
        let mn_details: Vec<_> = vote
            .governor_details
            .iter()
            .filter(|d| d.owner_type == NationType::MinorNation)
            .collect();

        if !mn_details.is_empty() {
            println!("    Minor Nation governor preferences:");
            // Group by province owner for cleaner display
            let mut shown_owners: std::collections::HashSet<NationId> =
                std::collections::HashSet::new();
            for detail in &mn_details {
                if shown_owners.insert(detail.province_owner) {
                    let owner_name = game
                        .get_nation(detail.province_owner)
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown");
                    let voted_for_name = game
                        .get_nation(detail.voted_for)
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown");
                    println!(
                        "      Governor of {} votes for {} ({})",
                        owner_name, voted_for_name, detail.reason
                    );
                }
            }
            println!();
        }

        // Show vote tally
        println!("    Vote tally:");
        for (nid, count) in &vote.votes {
            let name = game
                .get_nation(*nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let marker = if vote.winner == Some(*nid) {
                " <-- WINNER"
            } else {
                ""
            };
            println!("      {:<12} {:>3} votes{}", name, count, marker);
        }
        println!(
            "    Threshold for majority: {} / {}",
            vote.majority_threshold, vote.total_governors
        );
        println!();

        // Show which GP had most trade influence over MN governors
        let mut mn_influence: std::collections::HashMap<NationId, u32> =
            std::collections::HashMap::new();
        for detail in &mn_details {
            *mn_influence.entry(detail.voted_for).or_insert(0) += 1;
        }
        if !mn_influence.is_empty()
            && let Some((&most_influential, &mn_votes)) =
                mn_influence.iter().max_by_key(|&(_, v)| *v)
        {
            let name = game
                .get_nation(most_influential)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            println!(
                "    Most trade influence over Minor Nations: {} ({} MN governors)",
                name, mn_votes
            );
        }
        println!();
    }

    // Final diplomatic standings
    println!("  Diplomatic Standings:");
    for gp in game.great_powers() {
        let standing = game.diplomacy.get_standing(gp.id);
        let marker = if gp.id == game.human_player_nation {
            " <-- YOU"
        } else {
            ""
        };
        println!("    {:<12} standing: {:>4}{}", gp.name, standing, marker);
    }
    println!();

    // Determine winner
    let winner_id = if let Some(ref vote) = report.council_vote {
        vote.winner
    } else {
        // No council vote in final report — winner is highest scorer
        scores.first().map(|(id, _, _, _)| *id)
    };

    if let Some(wid) = winner_id {
        let winner_name = game
            .get_nation(wid)
            .map(|n| n.name.as_str())
            .unwrap_or("Unknown");
        if wid == game.human_player_nation {
            println!(
                "  *** CONGRATULATIONS! {} (YOU) wins the game! ***",
                winner_name
            );
        } else {
            println!("  {} wins the game.", winner_name);
        }
    }

    // Show high scores
    if !game.high_scores.is_empty() {
        println!();
        println!("  High Scores:");
        for (i, (name, score, date)) in game.high_scores.iter().enumerate() {
            println!(
                "    {}. {} - {} ({})",
                i + 1,
                name,
                format_number(*score),
                date
            );
        }
    }

    println!();
    println!("  Play again? (start a new game with 'cargo run')");
}

fn print_nation_info(game: &GameState, query: &str) {
    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    let target_id = target.id;
    let player_id = game.human_player_nation;

    println!("  ══════════════════════════════════════");
    println!(
        "  {} ({})",
        target.name,
        if target.is_great_power() {
            "Great Power"
        } else {
            "Minor Nation"
        }
    );
    println!("  ══════════════════════════════════════");
    println!();

    // Province count and names
    let owned_provinces: Vec<_> = game
        .provinces
        .iter()
        .filter(|p| p.owner == target_id)
        .collect();
    println!("  Provinces: {}", owned_provinces.len());
    for p in &owned_provinces {
        let settlement = match p.settlement_level {
            domain::map::SettlementLevel::Hamlet => "Hamlet",
            domain::map::SettlementLevel::Village => "Village",
            domain::map::SettlementLevel::Town => "Town",
        };
        println!(
            "    - {} ({}, {} tiles)",
            p.name,
            settlement,
            p.tile_count()
        );
    }
    println!();

    // Treasury (Great Powers only)
    if target.is_great_power() {
        println!("  Treasury: {}", target.treasury);
    }

    // AI personality (for AI-controlled Great Powers)
    if target_id != player_id
        && let Some(personality) = target.ai_personality
    {
        println!("  AI Personality: {}", personality);
    }

    // Army size and total firepower
    println!("  Army: {} units", target.army.len());
    if !target.army.is_empty() {
        println!(
            "  Total firepower: {:.1}",
            target.total_military_firepower()
        );
    }
    println!();

    // Diplomatic relations with player
    if target_id != player_id {
        let status = match game.diplomacy.get_relation(player_id, target_id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
                } else if target.is_great_power() {
                    if rel.has_treaty(domain::events::TreatyType::Alliance) {
                        format!("Allied (score: {})", rel.score)
                    } else {
                        format!("Neutral (score: {})", rel.score)
                    }
                } else if rel.has_embassy {
                    format!("Embassy (score: {})", rel.score)
                } else if rel.has_consulate {
                    format!("Consulate (score: {})", rel.score)
                } else {
                    format!("No relations (score: {})", rel.score)
                }
            }
            None => "No contact".to_string(),
        };
        println!("  Relations with you: {}", status);
    } else {
        println!("  (This is your nation)");
    }

    // Score (Great Powers only)
    if target.is_great_power() {
        let score = calculate_score(target);
        println!(
            "  Score: {} (Mil:{} Lab:{} Trans:{} Dip:{} Prov:{})",
            format_number(score.total),
            score.military_score,
            score.labor_score,
            score.transport_score,
            score.diplomatic_score,
            score.province_score,
        );
    }
}

fn read_line() -> String {
    let mut input = String::new();
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut input).unwrap_or(0);
    input
}

fn print_prompt(game: &domain::game_state::GameState) {
    let nation = game.get_nation(game.human_player_nation).unwrap();
    print!(
        "  [{} | {} | {}] > ",
        nation.name, game.turn, nation.treasury
    );
}

fn print_status(game: &domain::game_state::GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    println!("  Playing as: {} (Great Power)", player.name);
    println!("  Turn: {} (Year {})", game.turn, game.turn.year());
    println!("  Treasury: {}", player.treasury);
    println!("  Provinces: {}", player.province_count());
    println!("  Difficulty: {:?}", game.difficulty);
}

fn print_warehouse(game: &domain::game_state::GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    println!("  WAREHOUSE:");

    let mut has_any = false;

    // Resources
    let resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
        ResourceType::Oil,
        ResourceType::Gold,
        ResourceType::Gems,
    ];
    println!("    Raw Resources:");
    for r in &resources {
        let amt = player.resource_amount(*r);
        if amt > 0 {
            println!("      {:?}: {}", r, amt);
            has_any = true;
        }
    }

    // Materials
    let materials = [
        MaterialType::Lumber,
        MaterialType::Steel,
        MaterialType::Fabric,
        MaterialType::Paper,
        MaterialType::Arms,
        MaterialType::CannedFood,
    ];
    println!("    Materials:");
    for m in &materials {
        let amt = player.materials.get(m).copied().unwrap_or(0);
        if amt > 0 {
            println!("      {:?}: {}", m, amt);
            has_any = true;
        }
    }

    // Goods
    let goods = [
        GoodsType::Furniture,
        GoodsType::Clothing,
        GoodsType::Hardware,
    ];
    println!("    Finished Goods:");
    for g in &goods {
        let amt = player.goods.get(g).copied().unwrap_or(0);
        if amt > 0 {
            println!("      {:?}: {}", g, amt);
            has_any = true;
        }
    }

    if !has_any {
        println!("    (empty — end your first turn to begin gathering resources)");
    }
}

fn print_provinces(game: &domain::game_state::GameState) {
    println!("  YOUR PROVINCES:");
    for province in &game.provinces {
        if province.owner == game.human_player_nation {
            let mut terrain_counts: std::collections::BTreeMap<char, u32> =
                std::collections::BTreeMap::new();
            for tile_coord in &province.tiles {
                if let Some(tile) = game.hex_map.get_tile(*tile_coord) {
                    *terrain_counts
                        .entry(terrain_char(tile.terrain()))
                        .or_insert(0) += 1;
                }
            }
            let terrain_summary: String = terrain_counts
                .iter()
                .map(|(ch, count)| format!("{}x{}", count, ch))
                .collect::<Vec<_>>()
                .join(" ");

            let settlement = match province.settlement_level {
                domain::map::SettlementLevel::Hamlet => "Hamlet",
                domain::map::SettlementLevel::Village => "Village",
                domain::map::SettlementLevel::Town => "Town",
            };
            println!(
                "    {} ({} tiles, {}) [{}]",
                province.name,
                province.tile_count(),
                settlement,
                terrain_summary
            );
        }
    }
}

fn print_nations(game: &domain::game_state::GameState) {
    println!("  GREAT POWERS:");
    for nation in game.great_powers() {
        let marker = if nation.id == game.human_player_nation {
            " ◄ YOU"
        } else {
            ""
        };
        println!(
            "    {:>2}. {:<12} {:>2} provinces  {}{}",
            nation.id.0,
            nation.name,
            nation.province_count(),
            nation.treasury,
            marker
        );
    }
    println!();
    println!("  MINOR NATIONS:");
    for nation in game.minor_nations() {
        println!(
            "    {:>2}. {:<12} {:>2} provinces",
            nation.id.0,
            nation.name,
            nation.province_count(),
        );
    }
}

fn print_buildings(game: &domain::game_state::GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    println!("  BUILDINGS:");
    if player.buildings.is_empty() {
        println!("    (none)");
    } else {
        for b in &player.buildings {
            let pending = if b.pending_capacity > 0 {
                format!(
                    " (+{} in {} turn{})",
                    b.pending_capacity,
                    b.turns_until_upgrade,
                    if b.turns_until_upgrade != 1 { "s" } else { "" }
                )
            } else {
                String::new()
            };
            println!(
                "    {:?}: capacity {}{}",
                b.building_type, b.capacity, pending
            );
        }
    }
    println!();
    println!(
        "  Workers: {} untrained, {} trained, {} expert",
        player.labor.untrained, player.labor.trained, player.labor.expert
    );
}

fn print_population(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();

    let untrained = player.labor.untrained;
    let trained = player.labor.trained;
    let expert = player.labor.expert;
    let total = player.labor.total_workers();

    println!("  POPULATION:");
    println!("    Untrained workers: {}", untrained);
    println!("    Trained workers:   {}", trained);
    println!("    Expert workers:    {}", expert);
    println!("    Total:             {}", total);
    println!();

    // Food requirement
    let food_needed = total;
    let grain = player.resource_amount(ResourceType::Grain);
    let fruit = player.resource_amount(ResourceType::Fruit);
    let livestock = player.resource_amount(ResourceType::Livestock);
    let total_food = grain + fruit + livestock;

    println!("  FOOD:");
    println!(
        "    Available: {} grain, {} fruit, {} livestock ({} total)",
        grain, fruit, livestock, total_food
    );
    println!("    Requirement: {} per turn (1 per worker)", food_needed);
    if total_food >= food_needed {
        println!("    Status: SURPLUS (+{})", total_food - food_needed);
    } else {
        println!(
            "    Status: DEFICIT (-{}) -- workers will starve!",
            food_needed - total_food
        );
    }
    println!();

    // Recruitment capacity
    let max_recruits = max_recruitment_capacity(player);
    println!("  RECRUITMENT:");
    println!(
        "    Max workers: {} (based on {} provinces)",
        max_recruits,
        player.province_count()
    );
    println!("    Current workers: {} / {}", total, max_recruits);
    let has_expanded_capitol = player
        .buildings
        .iter()
        .any(|b| b.building_type == BuildingType::Capitol && b.capacity > 1);
    if has_expanded_capitol {
        println!("    Capitol expanded: 1 worker per 3 provinces");
    } else {
        println!("    Capitol base: 1 worker per 4 provinces (expand Capitol for 1 per 3)");
    }
}

fn print_tech(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    let year = game.turn.year();

    // Show already researched technologies
    println!("  RESEARCHED TECHNOLOGIES:");
    if player.researched_techs.is_empty() {
        println!("    (none)");
    } else {
        for tech_id in &player.researched_techs {
            if let Some(tech) = game.game_data.tech_tree.get(*tech_id) {
                println!("    [x] {}", tech.name);
            }
        }
    }
    println!();

    // Show available technologies
    let available = game
        .game_data
        .tech_tree
        .available_techs(&player.researched_techs, year);
    println!("  AVAILABLE FOR RESEARCH (year {}):", year);
    if available.is_empty() {
        println!("    (none available this year)");
    } else {
        for tech in &available {
            println!("    [ ] {} (cost: {})", tech.name, tech.cost);
        }
    }
    println!();
    println!("  Use 'research <name>' to research a technology.");
}

fn research_tech(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;
    let year = game.turn.year();

    let query_lower = query.to_lowercase();

    // Get available techs and find a case-insensitive partial match
    let player = game.get_nation(player_id).unwrap();
    let available = game
        .game_data
        .tech_tree
        .available_techs(&player.researched_techs, year);

    let matched: Vec<_> = available
        .iter()
        .filter(|t| t.name.to_lowercase().contains(&query_lower))
        .collect();

    match matched.len() {
        0 => {
            println!("  No available technology matches '{}'.", query);
            println!("  Use 'tech' to see what is available.");
        }
        1 => {
            let tech = matched[0];
            let player = game.get_nation(player_id).unwrap();

            // Check if player can afford it
            if player.treasury.checked_sub(tech.cost).is_none() {
                println!(
                    "  Cannot afford {} (cost: {}, treasury: {}).",
                    tech.name, tech.cost, player.treasury
                );
                return;
            }

            let tech_id = tech.id;
            let tech_name = tech.name.clone();
            let tech_cost = tech.cost;

            // Deduct cost and add tech
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= tech_cost;
            player.research_tech(tech_id);

            let player_name = player.name.clone();
            println!("  {}", color_green(&format!("Researched: {}!", tech_name)));
            println!("  Cost: {} (treasury now: {})", tech_cost, player.treasury);

            // Record history event (deduplicate: skip if same text already exists for this turn)
            let turn = game.turn;
            let entry_text = format!("{} researched {}", player_name, tech_name);
            if !game
                .history
                .iter()
                .any(|(t, text)| *t == turn && text == &entry_text)
            {
                game.history.push((turn, entry_text));
            }
        }
        _ => {
            println!(
                "  Multiple technologies match '{}'. Be more specific:",
                query
            );
            for tech in &matched {
                println!("    - {} (cost: {})", tech.name, tech.cost);
            }
        }
    }
}

fn print_trade(game: &GameState) {
    use domain::economy::trade;

    let player = game.get_nation(game.human_player_nation).unwrap();
    let cargo_capacity = player.total_cargo_capacity();

    println!("  TRADE STATUS:");
    println!(
        "    Merchant fleet: {} ships, {} cargo holds",
        player.merchant_ship_count(),
        cargo_capacity
    );
    println!();

    let offers = trade::generate_minor_nation_offers(&game.nations, &game.provinces, &game.hex_map);

    if offers.is_empty() {
        println!("  No trade offerings available from Minor Nations.");
        return;
    }

    // Group offers by seller
    let mut by_seller: std::collections::BTreeMap<String, (NationId, Vec<&trade::TradeOffer>)> =
        std::collections::BTreeMap::new();
    for offer in &offers {
        let seller_name = game
            .get_nation(offer.seller)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("Nation {}", offer.seller.0));
        let entry = by_seller
            .entry(seller_name)
            .or_insert((offer.seller, Vec::new()));
        entry.1.push(offer);
    }

    println!("  MINOR NATION TRADE OFFERINGS:");
    for (name, (seller_id, nation_offers)) in &by_seller {
        // Check consulate status
        let has_consulate = game
            .diplomacy
            .get_relation(game.human_player_nation, *seller_id)
            .is_some_and(|r| r.has_consulate);
        let status = if has_consulate {
            color_green("[Consulate]")
        } else {
            color_red("[No Consulate]")
        };
        println!("    {} {}:", name, status);
        let mut sorted_offers: Vec<_> = nation_offers.iter().collect();
        sorted_offers.sort_by_key(|o| format!("{:?}", o.resource));
        for offer in sorted_offers {
            println!(
                "      {:?}: {} available at {} each",
                offer.resource, offer.quantity, offer.price_per_unit
            );
        }
    }
    println!();

    // Show active subsidies
    if !player.trade_subsidies.is_empty() {
        println!("  ACTIVE TRADE SUBSIDIES:");
        let mut subsidy_entries: Vec<_> = player.trade_subsidies.iter().collect();
        subsidy_entries.sort_by_key(|(nid, _)| nid.0);
        for (target_id, amount) in subsidy_entries {
            let target_name = game
                .get_nation(*target_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Nation {}", target_id.0));
            println!("    {:<12} {} per turn", target_name, amount);
        }
        let total_subsidy: domain::types::Money = player
            .trade_subsidies
            .values()
            .copied()
            .fold(domain::types::Money::ZERO, |acc, v| acc + v);
        println!("    Total subsidy cost: {} per turn", total_subsidy);
        println!();
    }

    // Show cargo utilization: estimate how many holds are used by current trade volume
    if cargo_capacity > 0 {
        // Count total quantity being traded (sum of last turn's transactions for this player)
        let cargo_used: u32 = player
            .trade_history
            .iter()
            .filter(|th| th.turn == game.turn || th.turn.0 + 1 == game.turn.0)
            .filter(|th| th.partner != player.id) // buyer entries
            .map(|th| th.quantity)
            .sum();
        let cargo_used = cargo_used.min(cargo_capacity);
        println!("  Cargo: {}/{} holds used", cargo_used, cargo_capacity);
        println!("  (Trade is auto-resolved each turn — requires consulate + cargo capacity)");
    } else {
        println!(
            "  {} No cargo capacity! Build merchant ships to enable trade.",
            color_red("WARNING:")
        );
    }

    // Show recent trade history (last 10 entries)
    if !player.trade_history.is_empty() {
        println!();
        println!("  RECENT TRADE HISTORY:");
        let history_len = player.trade_history.len();
        let start = history_len.saturating_sub(10);
        for entry in &player.trade_history[start..] {
            let partner_name = game
                .get_nation(entry.partner)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Nation {}", entry.partner.0));
            println!(
                "    {} — {:?} x{} with {} for {}",
                entry.turn, entry.resource, entry.quantity, partner_name, entry.total_cost
            );
        }
    }
}

fn print_civilians(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    if player.civilians.is_empty() {
        println!("  No civilian units. Use 'hire <type>' to hire one.");
        println!("  Types: prospector, miner, engineer, farmer, rancher, forester, driller");
        return;
    }
    println!("  CIVILIAN UNITS:");
    for (i, c) in player.civilians.iter().enumerate() {
        let pos_str = match c.position {
            Some(coord) => format!("({}, {})", coord.q, coord.r),
            None => "Undeployed".to_string(),
        };
        let status_str = if c.working {
            format!("Working ({} turns left)", c.turns_remaining)
        } else {
            "Idle".to_string()
        };
        println!(
            "    [{}] {} — {} — {}",
            i, c.civilian_type, pos_str, status_str
        );
    }
}

fn cmd_hire_civilian(game: &mut GameState, type_name: &str) {
    if check_bankrupt(game) {
        return;
    }

    let civ_type = match parse_civilian_type(type_name) {
        Some(ct) => ct,
        None => {
            println!("  Unknown civilian type: '{}'", type_name);
            println!("  Available: prospector ($100), miner ($1500), engineer ($500),");
            println!(
                "             farmer ($100), rancher ($100), forester ($100), driller ($2000)"
            );
            return;
        }
    };

    let cost = civ_type.creation_cost();
    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if player.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford {} (cost: {}, treasury: {}).",
            civ_type, cost, player.treasury
        );
        return;
    }

    let id = next_civilian_id();
    let civilian = domain::economy::civilians::Civilian::new(id, civ_type, player_id);

    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= cost;
    player.civilians.push(civilian);

    println!(
        "  Hired {} (cost: {}, treasury now: {}). Use 'deploy <index> <province>' to deploy.",
        civ_type, cost, player.treasury
    );
}

fn cmd_deploy_civilian(game: &mut GameState, args: &str) {
    // Parse: "<index> <province_name>"
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: deploy <civilian_index> <province_name>");
        println!("  Example: deploy 0 France City");
        return;
    }
    let index: usize = match parts[0].parse() {
        Ok(i) => i,
        Err(_) => {
            println!(
                "  Invalid index: '{}'. Use 'civilians' to see indices.",
                parts[0]
            );
            return;
        }
    };
    let province_name = parts[1].trim();

    let player_id = game.human_player_nation;

    // Phase 1: gather info with immutable borrows, extract owned data
    let (civ_type, coord, prov_name) = {
        let player = game.get_nation(player_id).unwrap();

        if index >= player.civilians.len() {
            println!(
                "  Invalid index {}. You have {} civilians.",
                index,
                player.civilians.len()
            );
            return;
        }

        let civ_type = player.civilians[index].civilian_type;

        // Find the province by name (case-insensitive partial match)
        let lower_name = province_name.to_lowercase();
        let matching_provinces: Vec<_> = game
            .provinces
            .iter()
            .filter(|p| p.owner == player_id && p.name.to_lowercase().contains(&lower_name))
            .collect();

        let province = match matching_provinces.len() {
            0 => {
                println!("  No owned province matches '{}'.", province_name);
                return;
            }
            1 => matching_provinces[0],
            _ => {
                println!(
                    "  Multiple provinces match '{}'. Be more specific.",
                    province_name
                );
                return;
            }
        };

        let prov_name = province.name.clone();

        // Find the first improvable tile in the province for this civilian type
        let mut target_coord = None;
        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && civ_type.can_improve(tile.terrain())
                && tile.improvement_level() < tile.terrain().max_improvement_level()
                && tile.assigned_civilian.is_none()
            {
                target_coord = Some(*tile_coord);
                break;
            }
        }

        let coord = match target_coord {
            Some(c) => c,
            None => {
                println!(
                    "  No improvable tile for {} in province '{}'.",
                    civ_type, prov_name
                );
                return;
            }
        };

        (civ_type, coord, prov_name)
    };

    // Phase 2: apply mutations
    let player = game.get_nation_mut(player_id).unwrap();
    let civilian = &mut player.civilians[index];
    civilian.deploy(coord);
    civilian.start_work(1); // 1 turn for improvements
    let civ_id = civilian.id;

    // Assign civilian to tile
    if let Some(tile) = game.hex_map.get_tile_mut(coord) {
        tile.assigned_civilian = Some(civ_id);
    }

    println!(
        "  Deployed {} to ({}, {}) in province '{}'. Work will complete next turn.",
        civ_type, coord.q, coord.r, prov_name
    );
}

fn print_help() {
    println!("  {}", color_bold("ECONOMY:"));
    println!("    warehouse         — Show your resource warehouse");
    println!("    buildings         — Show your buildings and workers");
    println!("    population / pop  — Show population, food balance, recruitment capacity");
    println!("    transport         — Show your transport system (freight cars, capacity)");
    println!("    trade             — Show Minor Nation trade offerings and subsidies");
    println!("    subsidy <n> <$>   — Set trade subsidy with Minor Nation (0 to remove)");
    println!("    build <building>  — Build a new mill or factory");
    println!("    expand <building> — Expand an existing building's capacity");
    println!("    recruit           — Recruit an untrained worker");
    println!("    train             — Train an untrained worker to trained");
    println!("    build car         — Build a freight car");
    println!("    build ship <type> — Build a merchant ship (trader, indiaman)");
    println!("    fleet             — Show your merchant fleet");
    println!("    sell <res> <qty>  — Sell resources at base price");
    println!();
    println!("  {}", color_bold("MILITARY:"));
    println!("    military / army   — Show your army units and their stats");
    println!("    build unit <type> — Build a military unit");
    println!("    upgrade <index>   — Upgrade army unit #i to its next type ($500)");
    println!("    move <i> <prov>   — Move army unit #i to a province you own");
    println!("    attack <nation>   — Order an attack on a nation you are at war with");
    println!("    navy              — Show your warship fleet");
    println!("    build warship <t> — Build a warship (frigate, ship-of-the-line)");
    println!("    blockade <nation> — Show blockade status against an enemy nation");
    println!("    produce arms      — Convert 1 steel into 1 arms");
    println!();
    println!("  {}", color_bold("DIPLOMACY:"));
    println!("    diplomacy         — Show diplomatic relations with all nations");
    println!("    consulate <name>  — Build a trade consulate with a Minor Nation ($500)");
    println!("    embassy <name>    — Build an embassy with a Minor Nation ($5,000)");
    println!("    pact <name>       — Propose non-aggression pact with a Minor Nation");
    println!("    alliance <name>   — Propose alliance with a Great Power");
    println!("    grant <name> <$>  — Send cash grant to improve relations");
    println!("    war <name>        — Declare war on a nation");
    println!("    peace <name>      — Propose peace with a nation you are at war with");
    println!();
    println!("  {}", color_bold("CIVILIANS:"));
    println!("    civilians         — List your civilian units (type, position, status)");
    println!("    hire <type>       — Hire a civilian specialist");
    println!("    deploy <i> <prov> — Deploy civilian #i to a province to start working");
    println!();
    println!("  {}", color_bold("TECHNOLOGY:"));
    println!("    tech              — Show technology tree status");
    println!("    research <name>   — Research a technology by name");
    println!();
    println!("  {}", color_bold("MAP:"));
    println!("    map               — Show the world map");
    println!("    provinces         — Show your provinces");
    println!("    infra             — Show infrastructure (railroads, depots, ports)");
    println!("    info <name>       — Show detailed info about any nation");
    println!("    nations           — Show all nations");
    println!("    score             — Show scores for all Great Powers");
    println!();
    println!("  {}", color_bold("INFRASTRUCTURE:"));
    println!("    build railroad    — Build a railroad in your capital province");
    println!("    build depot       — Build a depot on your capital tile ($2,000)");
    println!("    build port        — Build a port on a coastal tile ($3,000)");
    println!("    build fort [prov] — Build/upgrade a fort (capital if no province given)");
    println!();
    println!("  {}", color_bold("GAME:"));
    println!("    [Enter] / turn    — End turn (gather resources, advance time)");
    println!("    orders / pending  — Show pending orders before turn end");
    println!("    auto <turns>      — Fast-forward N turns with minimal output");
    println!("    overview          — Comprehensive empire overview");
    println!("    history           — Show timeline of major events");
    println!("    save              — Save the current game (shows existing saves)");
    println!("    load              — List saved games");
    println!("    load <filename>   — Load a saved game");
    println!("    delete <filename> — Delete a saved game");
    println!("    saveinfo <file>   — Show save file metadata without loading");
    println!("    quicksave / qs    — Quick save to quicksave.json");
    println!("    quickload / ql    — Quick load from quicksave.json");
    println!("    help              — Show this help");
    println!("    quit              — Exit the game");
}

fn saves_dir() -> PathBuf {
    PathBuf::from("saves")
}

fn save_current_game(game: &GameState) {
    let dir = saves_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        println!("  Failed to create saves directory: {}", e);
        return;
    }

    // Show existing saves before saving
    print_save_list(&dir);

    let filename = format!("save_{}_Q{}.json", game.turn.year(), game.turn.quarter());
    let path = dir.join(&filename);

    match persistence::save_game(game, &path) {
        Ok(()) => {
            println!("  Game saved to: saves/{}", filename);
        }
        Err(e) => {
            println!("  Failed to save: {}", e);
        }
    }
}

fn quicksave_game(game: &GameState) {
    let dir = saves_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        println!("  Failed to create saves directory: {}", e);
        return;
    }
    let path = dir.join("quicksave.json");
    match persistence::save_game(game, &path) {
        Ok(()) => {
            println!("  Quicksave complete.");
        }
        Err(e) => {
            println!("  Quicksave failed: {}", e);
        }
    }
}

fn list_saved_games() {
    let dir = saves_dir();
    if !dir.exists() {
        println!("  No saved games found.");
        println!();
        println!("  Use: load <filename> (e.g., \"load save_1820_Q1.json\")");
        return;
    }

    println!();
    print_save_list(&dir);
    println!();
    println!("  Use: load <filename> (e.g., \"load save_1820_Q1.json\")");
}

fn print_save_list(dir: &std::path::Path) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect(),
        Err(_) => return,
    };

    if entries.is_empty() {
        return;
    }

    // Sort by modification time, most recent first
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

    println!("  SAVED GAMES:");
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let meta_str = if let Some(meta) = persistence::read_save_metadata(&entry.path()) {
            let size_str = entry
                .metadata()
                .map(|m| {
                    let bytes = m.len();
                    if bytes >= 1_048_576 {
                        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
                    } else if bytes >= 1024 {
                        format!("{:.0} KB", bytes as f64 / 1024.0)
                    } else {
                        format!("{} B", bytes)
                    }
                })
                .unwrap_or_default();
            let ts_display = if meta.timestamp.len() >= 16 {
                // Show "YYYY-MM-DD HH:MM" from ISO 8601
                meta.timestamp[..16].replace('T', " ")
            } else {
                meta.timestamp.clone()
            };
            format!(
                " ({}, {} {}, {})",
                size_str, meta.nation_name, meta.turn_display, ts_display
            )
        } else {
            let size_str = entry
                .metadata()
                .map(|m| {
                    let bytes = m.len();
                    if bytes >= 1_048_576 {
                        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
                    } else if bytes >= 1024 {
                        format!("{:.0} KB", bytes as f64 / 1024.0)
                    } else {
                        format!("{} B", bytes)
                    }
                })
                .unwrap_or_default();
            format!(" ({})", size_str)
        };
        println!("    {}. {}{}", i + 1, name_str, meta_str);
    }
}

fn load_saved_game(filename: &str) -> Result<GameState, String> {
    let dir = saves_dir();
    let path = dir.join(filename);
    persistence::load_game(&path)
}

fn delete_saved_game(filename: &str) {
    let dir = saves_dir();
    let path = dir.join(filename);
    match persistence::delete_save(&path) {
        Ok(()) => {
            println!("  Save deleted: {}", filename);
        }
        Err(e) => {
            println!("  Failed to delete: {}", e);
        }
    }
}

fn cmd_saveinfo(filename: &str) {
    let dir = saves_dir();
    let path = dir.join(filename);
    if !path.exists() {
        println!("  Save file not found: {}", filename);
        println!("  Use 'load' to list available saves.");
        return;
    }
    match persistence::read_save_metadata(&path) {
        Some(meta) => {
            println!();
            println!("  SAVE FILE INFO: {}", filename);
            println!("    Version:    {}", meta.version);
            println!("    Nation:     {}", meta.nation_name);
            println!("    Turn:       {}", meta.turn_display);
            println!("    Difficulty: {}", meta.difficulty);
            println!("    Timestamp:  {}", meta.timestamp);
        }
        None => {
            println!(
                "  Could not read metadata from '{}'. File may be corrupt or in an old format.",
                filename
            );
        }
    }
}

fn print_legend() {
    println!("  Legend: F=Farm f=Forest H=Hills M=Mountain ~=Sea .=Plains");
    println!("         P=Plantation R=Range h=HorseRanch O=Orchard");
    println!("         S=Swamp D=Desert T=Tundra s=Scrub  ★=Capital");
}

fn print_scores(game: &GameState) {
    println!("  NATION SCORES:");
    println!(
        "    {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}",
        "Nation", "Mil", "Labor", "Trans", "Diplo", "Prov", "TOTAL"
    );
    println!("    {}", "-".repeat(58));
    let mut scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| {
            let s = calculate_score(n);
            (n.id, n.name.clone(), s)
        })
        .collect();
    scores.sort_by(|a, b| b.2.total.cmp(&a.2.total));
    for (id, name, s) in &scores {
        let marker = if *id == game.human_player_nation {
            " ◄"
        } else {
            ""
        };
        println!(
            "    {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}{}",
            name,
            s.military_score,
            s.labor_score,
            s.transport_score,
            s.diplomatic_score,
            s.province_score,
            s.total,
            marker
        );
    }
}

fn terrain_char(terrain: TerrainType) -> char {
    match terrain {
        TerrainType::Farm => 'F',
        TerrainType::HardwoodForest => 'f',
        TerrainType::ScrubForest => 's',
        TerrainType::FertileHills => 'H',
        TerrainType::BarrenHills => 'h',
        TerrainType::Mountain => 'M',
        TerrainType::Sea => '~',
        TerrainType::DryPlains => '.',
        TerrainType::Plantation => 'P',
        TerrainType::OpenRange => 'R',
        TerrainType::HorseRanch => 'r',
        TerrainType::Orchard => 'O',
        TerrainType::Swamp => 'S',
        TerrainType::Desert => 'D',
        TerrainType::Tundra => 'T',
    }
}

fn nation_color_code(nation: &Nation) -> &str {
    use domain::nation::NationColor;
    match nation.color {
        NationColor::Yellow => "\x1b[93m",
        NationColor::Orange => "\x1b[33m",
        NationColor::LightBlue => "\x1b[96m",
        NationColor::Red => "\x1b[91m",
        NationColor::Green => "\x1b[92m",
        NationColor::Purple => "\x1b[95m",
        NationColor::Blue => "\x1b[94m",
        NationColor::Gray => "\x1b[37m",
        NationColor::Brown => "\x1b[33m",
        NationColor::Pink => "\x1b[95m",
        NationColor::Teal => "\x1b[36m",
        NationColor::Olive => "\x1b[32m",
        NationColor::Maroon => "\x1b[31m",
        NationColor::Navy => "\x1b[34m",
        NationColor::Cyan => "\x1b[36m",
        NationColor::Lime => "\x1b[92m",
        NationColor::Coral => "\x1b[91m",
        NationColor::Lavender => "\x1b[35m",
        NationColor::Tan => "\x1b[33m",
        NationColor::Salmon => "\x1b[91m",
        NationColor::Khaki => "\x1b[93m",
        NationColor::Indigo => "\x1b[34m",
    }
}

fn print_diplomacy(game: &GameState) {
    let player_id = game.human_player_nation;
    let standing = game.diplomacy.get_standing(player_id);

    println!("  DIPLOMATIC STATUS (Standing: {})", standing);
    println!();

    // Show Great Power relations
    println!("  GREAT POWERS:");
    for gp in game.great_powers() {
        if gp.id == player_id {
            continue;
        }
        let status = match game.diplomacy.get_relation(player_id, gp.id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
                } else if rel.has_treaty(domain::events::TreatyType::Alliance) {
                    format!("Allied (score: {})", rel.score)
                } else {
                    format!("Neutral (score: {})", rel.score)
                }
            }
            None => "No contact".to_string(),
        };
        println!("    {:<12} {}", gp.name, status);
    }
    println!();

    // Show Minor Nation relations
    let player = game.get_nation(player_id).unwrap();
    println!("  MINOR NATIONS:");
    for mn in game.minor_nations() {
        let status = match game.diplomacy.get_relation(player_id, mn.id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
                } else if rel.has_treaty(domain::events::TreatyType::NonAggressionPact) {
                    format!("Pact + Embassy (score: {})", rel.score)
                } else if rel.has_embassy {
                    format!("Embassy (score: {})", rel.score)
                } else if rel.has_consulate {
                    format!("Consulate (score: {})", rel.score)
                } else {
                    format!("No relations (score: {})", rel.score)
                }
            }
            None => "No relations".to_string(),
        };
        let subsidy_info = player
            .trade_subsidies
            .get(&mn.id)
            .filter(|s| **s != domain::types::Money::ZERO)
            .map(|s| format!(" [Subsidy: {}/turn]", s))
            .unwrap_or_default();
        println!("    {:<12} {}{}", mn.name, status, subsidy_info);
    }
}

fn cmd_consulate(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Consulates are for Minor Nations only.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    let player = game.get_nation(player_id).unwrap();
    let cost = Money::dollars(500);
    if player.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford consulate (cost: {}, treasury: {}).",
            cost, player.treasury
        );
        return;
    }

    match game.diplomacy.build_consulate(player_id, target_id) {
        Ok(_) => {
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= cost;
            println!(
                "  {}",
                color_green(&format!(
                    "Trade consulate established with {}! (cost: {}, treasury now: {})",
                    target_name, cost, player.treasury
                ))
            );

            // Record history event
            let turn = game.turn;
            game.history
                .push((turn, format!("Trade consulate built with {}", target_name)));
        }
        Err(e) => {
            println!("  Cannot build consulate: {}", e);
        }
    }
}

fn cmd_embassy(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Use diplomacy with Great Powers through treaties.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    let player = game.get_nation(player_id).unwrap();
    let cost = Money::dollars(5000);
    if player.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford embassy (cost: {}, treasury: {}).",
            cost, player.treasury
        );
        return;
    }

    match game.diplomacy.build_embassy(player_id, target_id) {
        Ok(_) => {
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= cost;
            println!(
                "  {}",
                color_green(&format!(
                    "Embassy established with {}! (cost: {}, treasury now: {})",
                    target_name, cost, player.treasury
                ))
            );

            // Record history event
            let turn = game.turn;
            game.history
                .push((turn, format!("Embassy built with {}", target_name)));
        }
        Err(e) => {
            println!("  Cannot build embassy: {}", e);
        }
    }
}

fn cmd_war(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.id == player_id {
        println!("  You cannot declare war on yourself.");
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    // Check if already at war
    if let Some(rel) = game.diplomacy.get_relation(player_id, target_id)
        && rel.at_war
    {
        println!("  You are already at war with {}.", target_name);
        return;
    }

    game.diplomacy.declare_war(player_id, target_id);
    let player_name = game
        .get_nation(player_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    println!();
    println!("  ╔════════════════════════════════════════╗");
    println!("  ║  DECLARATION OF WAR                    ║");
    println!("  ╚════════════════════════════════════════╝");
    println!(
        "  {}",
        color_red(&format!(
            "Your Excellency has declared WAR upon {}!",
            target_name
        ))
    );
    println!("  May Providence favor our cause.");
    println!();

    // Record history event
    let turn = game.turn;
    game.history.push((
        turn,
        format!("{} declared war on {}", player_name, target_name),
    ));
}

fn cmd_peace(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    let target_id = target.id;
    let target_name = target.name.clone();

    // Check if actually at war
    match game.diplomacy.get_relation(player_id, target_id) {
        Some(rel) if rel.at_war => {}
        _ => {
            println!("  You are not at war with {}.", target_name);
            return;
        }
    }

    game.diplomacy.make_peace(player_id, target_id);
    let player_name = game
        .get_nation(player_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    println!(
        "  {}",
        color_green(&format!("Peace has been established with {}.", target_name))
    );
    println!("  The cannons fall silent.");

    // Record history event
    let turn = game.turn;
    game.history.push((
        turn,
        format!("{} signed peace with {}", player_name, target_name),
    ));
}

fn cmd_pact(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Non-aggression pacts are for Minor Nations only.",
            target.name
        );
        println!(
            "  Use 'alliance {}' to form an alliance with a Great Power.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    match game.diplomacy.propose_pact(player_id, target_id) {
        Ok(()) => {
            println!(
                "  {}",
                color_green(&format!("Non-aggression pact signed with {}!", target_name))
            );
            let turn = game.turn;
            let player_name = game
                .get_nation(player_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            game.history.push((
                turn,
                format!(
                    "{} signed non-aggression pact with {}",
                    player_name, target_name
                ),
            ));
        }
        Err(e) => {
            println!("  Cannot propose pact: {}", e);
        }
    }
}

fn cmd_alliance(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if !target.is_great_power() {
        println!(
            "  {} is a Minor Nation. Alliances are between Great Powers only.",
            target.name
        );
        println!(
            "  Use 'pact {}' to sign a non-aggression pact.",
            target.name
        );
        return;
    }

    if target.id == player_id {
        println!("  You cannot form an alliance with yourself.");
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    match game.diplomacy.propose_alliance(player_id, target_id) {
        Ok(()) => {
            println!(
                "  {}",
                color_green(&format!("Alliance formed with {}!", target_name))
            );
            let turn = game.turn;
            let player_name = game
                .get_nation(player_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            game.history.push((
                turn,
                format!("{} formed an alliance with {}", player_name, target_name),
            ));
        }
        Err(e) => {
            println!("  Cannot form alliance: {}", e);
        }
    }
}

fn cmd_grant(game: &mut GameState, args: &str) {
    let player_id = game.human_player_nation;

    // Parse: grant <nation> <amount>
    let parts: Vec<&str> = args.rsplitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: grant <nation> <amount>");
        println!("  Example: grant Bavaria 500");
        return;
    }

    let amount_str = parts[0];
    let nation_query = parts[1].trim();

    let amount: i64 = match amount_str.parse() {
        Ok(v) if v > 0 => v,
        _ => {
            println!(
                "  Invalid amount '{}'. Must be a positive number.",
                amount_str
            );
            return;
        }
    };

    let target = match game.find_nation_by_name(nation_query) {
        Some(n) => n,
        None => {
            println!(
                "  No unique nation matches '{}'. Be more specific.",
                nation_query
            );
            return;
        }
    };

    let target_id = target.id;
    let target_name = target.name.clone();

    if target_id == player_id {
        println!("  You cannot send a grant to yourself.");
        return;
    }

    let grant = Money::dollars(amount);
    let player = game.get_nation(player_id).unwrap();
    if player.treasury.checked_sub(grant).is_none() {
        println!(
            "  Cannot afford grant of {} (treasury: {}).",
            grant, player.treasury
        );
        return;
    }

    game.diplomacy.send_grant(player_id, target_id, grant);
    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= grant;
    let new_treasury = player.treasury;
    let score = game
        .diplomacy
        .get_relation(player_id, target_id)
        .map(|r| r.score)
        .unwrap_or(0);

    println!(
        "  {}",
        color_green(&format!(
            "Sent ${} grant to {}. Relationship score now: {}. Treasury: {}",
            amount, target_name, score, new_treasury
        ))
    );
}

fn cmd_subsidy(game: &mut GameState, args: &str) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;

    // Parse: subsidy <nation> <amount>
    let parts: Vec<&str> = args.rsplitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: subsidy <nation> <amount>");
        println!("  Example: subsidy Bavaria 200");
        println!("  Set to 0 to remove: subsidy Bavaria 0");
        return;
    }

    let amount_str = parts[0];
    let nation_query = parts[1].trim();

    let amount: i64 = match amount_str.parse() {
        Ok(v) if v >= 0 => v,
        _ => {
            println!(
                "  Invalid amount '{}'. Must be a non-negative number.",
                amount_str
            );
            return;
        }
    };

    let target = match game.find_nation_by_name(nation_query) {
        Some(n) => n,
        None => {
            println!(
                "  No unique nation matches '{}'. Be more specific.",
                nation_query
            );
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Subsidies are for Minor Nations only.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    if amount == 0 {
        // Remove subsidy
        let player = game.get_nation_mut(player_id).unwrap();
        player.trade_subsidies.remove(&target_id);
        println!(
            "  {}",
            color_green(&format!("Trade subsidy with {} removed.", target_name))
        );
    } else {
        let subsidy = Money::dollars(amount);
        let player = game.get_nation_mut(player_id).unwrap();
        player.trade_subsidies.insert(target_id, subsidy);
        println!(
            "  {}",
            color_green(&format!(
                "Trade subsidy set: {} per turn to {}.",
                subsidy, target_name
            ))
        );
    }
}

fn cmd_attack(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.id == player_id {
        println!("  You cannot attack yourself.");
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    // Check at war
    let at_war = game
        .diplomacy
        .get_relation(player_id, target_id)
        .is_some_and(|rel| rel.at_war);
    if !at_war {
        println!(
            "  You are not at war with {}. Declare war first with 'war {}'.",
            target_name, target_name
        );
        return;
    }

    // Check player has army units
    let player = game.get_nation(player_id).unwrap();
    if player.army.is_empty() {
        println!("  You have no army units! Build units first with 'build unit <type>'.");
        return;
    }

    // Find first province owned by target
    let target_province = game.provinces.iter().find(|p| p.owner == target_id);

    let province_id = match target_province {
        Some(p) => p.id,
        None => {
            println!("  {} has no provinces to attack.", target_name);
            return;
        }
    };

    let province_name = game
        .get_province(province_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    game.pending_attacks.push((player_id, province_id));
    println!(
        "  Attack ordered! Your army will assault {} (province of {}) at end of turn.",
        province_name, target_name
    );
    println!(
        "  {} pending attack(s) queued. End turn to resolve.",
        game.pending_attacks.len()
    );
}

fn print_transport(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    let ts = &player.transport;
    println!("  TRANSPORT SYSTEM:");
    println!("    Freight cars: {}", ts.freight_cars);
    println!("    Total capacity: {} units", ts.total_capacity());
    println!(
        "    Military transport capacity: {} army units",
        ts.military_transport_capacity()
    );
    println!();

    // Show connected vs disconnected provinces
    let connected = domain::turn::connected_provinces(game, game.human_player_nation);
    let total_provinces = player.province_count();
    let connected_count = connected.len();
    let disconnected_count = total_provinces - connected_count;
    println!(
        "  Provinces: {} connected, {} disconnected (of {} total)",
        connected_count, disconnected_count, total_provinces
    );

    // Calculate total production from connected tiles
    let mut total_production: std::collections::HashMap<ResourceType, u32> =
        std::collections::HashMap::new();
    for province in &game.provinces {
        if province.owner != game.human_player_nation {
            continue;
        }
        if !connected.contains(&province.id) {
            continue;
        }
        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                *total_production.entry(yield_amount.resource).or_insert(0) +=
                    yield_amount.quantity;
            }
        }
    }

    let total_produced: u32 = total_production.values().sum();
    let capacity = ts.total_capacity();
    println!();
    println!("  RESOURCE PRODUCTION (connected tiles):");
    if total_production.is_empty() {
        println!("    (none)");
    } else {
        let mut sorted: Vec<_> = total_production.iter().collect();
        sorted.sort_by_key(|(r, _)| format!("{:?}", r));
        for (resource, amount) in &sorted {
            println!("    {:?}: {}", resource, amount);
        }
    }
    println!();
    println!(
        "  Total production: {} | Capacity: {} | {}",
        total_produced,
        capacity,
        if total_produced <= capacity {
            format!("Surplus capacity: {}", capacity - total_produced)
        } else {
            format!(
                "DEFICIT: {} resources will overflow",
                total_produced - capacity
            )
        }
    );

    println!();
    let (labor, lumber, steel) =
        domain::economy::transport::TransportSystem::build_freight_car_cost();
    println!(
        "  Use 'build car' to build a freight car (cost: {} labor, {} lumber, {} steel).",
        labor, lumber, steel
    );
}

fn build_freight_car(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    let (labor_needed, lumber_needed, steel_needed) =
        domain::economy::transport::TransportSystem::build_freight_car_cost();

    let total_labor = player.labor.total_workers();
    let lumber_have = player.material_amount(MaterialType::Lumber);
    let steel_have = player.material_amount(MaterialType::Steel);

    if total_labor < labor_needed {
        println!(
            "  Cannot build freight car: need {} labor (have {} workers).",
            labor_needed, total_labor
        );
        return;
    }
    if lumber_have < lumber_needed {
        println!(
            "  Cannot build freight car: need {} lumber (have {}).",
            lumber_needed, lumber_have
        );
        return;
    }
    if steel_have < steel_needed {
        println!(
            "  Cannot build freight car: need {} steel (have {}).",
            steel_needed, steel_have
        );
        return;
    }

    let player = game.get_nation_mut(player_id).unwrap();
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Steel, steel_needed);
    // Labor is consumed as a workforce requirement, not permanently removed.
    // (Workers are available each turn; this just requires having enough.)
    player.transport.build_freight_cars(1);

    println!(
        "  Freight car built! (consumed {} lumber, {} steel). Total cars: {}, capacity: {}.",
        lumber_needed,
        steel_needed,
        player.transport.freight_cars,
        player.transport.total_capacity()
    );
}

fn print_military(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();

    println!("  ARMY ({} units):", player.army.len());
    if player.army.is_empty() {
        println!("    (no units -- use 'build unit <type>' to recruit)");
    } else {
        for (i, unit) in player.army.iter().enumerate() {
            let province_name = game
                .get_province(unit.position)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!(
                "    [{}] {:?} (HP: {}, Medals: {}, FP: {:.1}) at {}",
                i,
                unit.unit_type,
                unit.health,
                unit.medals,
                unit.effective_firepower(),
                province_name
            );
        }
        println!();
        println!(
            "  Total firepower: {:.1}",
            player.total_military_firepower()
        );
    }

    if !game.pending_attacks.is_empty() {
        println!();
        println!("  PENDING ATTACKS:");
        for (attacker_id, province_id) in &game.pending_attacks {
            if *attacker_id == game.human_player_nation {
                let province_name = game
                    .get_province(*province_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Unknown");
                println!("    -> {}", province_name);
            }
        }
    }
}

fn cmd_build_railroad(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();
    let capital_province_id = player.capital_province_id;

    // Find the capital province to get its tiles
    let province = game.get_province(capital_province_id).unwrap();
    let tiles: Vec<HexCoord> = province.tiles.clone();

    // Find the first tile in the capital province that does not have a railroad
    let mut target_coord = None;
    for coord in &tiles {
        if let Some(tile) = game.hex_map.get_tile(*coord)
            && tile.terrain().is_land()
            && !tile.infrastructure.has_railroad
        {
            target_coord = Some(*coord);
            break;
        }
    }

    let coord = match target_coord {
        Some(c) => c,
        None => {
            println!("  All land tiles in your capital province already have railroads.");
            return;
        }
    };

    let terrain = game.hex_map.get_tile(coord).unwrap().terrain();
    let cost = match infrastructure::railroad_cost(terrain) {
        Some(c) => c,
        None => {
            println!("  Cannot build railroad on {:?}.", terrain);
            return;
        }
    };

    let treasury = game.get_nation(player_id).unwrap().treasury;
    if treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford railroad on {:?} at ({},{}) (cost: {}, treasury: {}).",
            terrain, coord.q, coord.r, cost, treasury
        );
        return;
    }

    match infrastructure::build_railroad(&mut game.hex_map, coord) {
        Ok(cost) => {
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= cost;
            println!(
                "  Railroad built on {:?} at ({},{})! Cost: {}, treasury now: {}.",
                terrain, coord.q, coord.r, cost, player.treasury
            );
        }
        Err(e) => {
            println!("  Cannot build railroad: {}", e);
        }
    }
}

fn cmd_build_depot(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();
    let capital_province_id = player.capital_province_id;
    let treasury = player.treasury;

    let province = game.get_province(capital_province_id).unwrap();
    let capital_tile_coord = province.capital_tile;

    let depot_cost = Money::dollars(2000);
    if treasury.checked_sub(depot_cost).is_none() {
        println!(
            "  Cannot afford depot (cost: {}, treasury: {}).",
            depot_cost, treasury
        );
        return;
    }

    match infrastructure::build_depot(&mut game.hex_map, capital_tile_coord) {
        Ok(cost) => {
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= cost;
            println!(
                "  Depot built at capital ({},{})! Cost: {}, treasury now: {}.",
                capital_tile_coord.q, capital_tile_coord.r, cost, player.treasury
            );
        }
        Err(e) => {
            println!("  Cannot build depot: {}", e);
        }
    }
}

fn cmd_build_port(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();
    let capital_province_id = player.capital_province_id;

    let province = game.get_province(capital_province_id).unwrap();
    let tiles: Vec<HexCoord> = province.tiles.clone();

    // Find the first coastal tile in the capital province without a port
    let mut target_coord = None;
    for coord in &tiles {
        if let Some(tile) = game.hex_map.get_tile(*coord)
            && tile.terrain().is_land()
            && !tile.infrastructure.has_port
        {
            let is_coastal = coord.neighbors().iter().any(|n| {
                game.hex_map
                    .get_tile(*n)
                    .is_some_and(|t| !t.terrain().is_land())
            });
            if is_coastal {
                target_coord = Some(*coord);
                break;
            }
        }
    }

    let coord = match target_coord {
        Some(c) => c,
        None => {
            println!("  No coastal tile without a port found in your capital province.");
            return;
        }
    };

    let port_cost = Money::dollars(3000);
    let treasury = game.get_nation(player_id).unwrap().treasury;
    if treasury.checked_sub(port_cost).is_none() {
        println!(
            "  Cannot afford port (cost: {}, treasury: {}).",
            port_cost, treasury
        );
        return;
    }

    match infrastructure::build_port(&mut game.hex_map, coord) {
        Ok(cost) => {
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= cost;
            println!(
                "  Port built at ({},{})! Cost: {}, treasury now: {}.",
                coord.q, coord.r, cost, player.treasury
            );
        }
        Err(e) => {
            println!("  Cannot build port: {}", e);
        }
    }
}

fn cmd_build_fort(game: &mut GameState, province_query: Option<&str>) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    // Find the target province: use specified province or fall back to capital
    let target_province_id = if let Some(query) = province_query {
        let lower = query.to_lowercase();
        let matches: Vec<_> = game
            .provinces
            .iter()
            .filter(|p| p.owner == player_id && p.name.to_lowercase().contains(&lower))
            .collect();
        match matches.len() {
            0 => {
                println!("  No owned province matches '{}'.", query);
                return;
            }
            1 => matches[0].id,
            _ => {
                println!("  Multiple provinces match '{}'. Be more specific.", query);
                return;
            }
        }
    } else {
        player.capital_province_id
    };

    let province = game.get_province(target_province_id).unwrap();
    let capital_tile_coord = province.capital_tile;
    let province_name = province.name.clone();

    // Check current fort level
    let current_level = game
        .hex_map
        .get_tile(capital_tile_coord)
        .map(|t| t.infrastructure.fort_level)
        .unwrap_or(0);
    let next_level = current_level + 1;

    if next_level > 3 {
        println!("  Fort in {} already at maximum level (3).", province_name);
        return;
    }

    let cost = domain::map::fort_cost(next_level).unwrap();
    let treasury = game.get_nation(player_id).unwrap().treasury;
    if treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford fort level {} in {} (cost: {}, treasury: {}).",
            next_level, province_name, cost, treasury
        );
        return;
    }

    match domain::map::build_fort(&mut game.hex_map, capital_tile_coord) {
        Ok((level, cost)) => {
            let player = game.get_nation_mut(player_id).unwrap();
            player.treasury -= cost;
            println!(
                "  {}",
                color_green(&format!(
                    "Fort in {} upgraded to level {}! Cost: {}, treasury now: {}.",
                    province_name, level, cost, player.treasury
                ))
            );
        }
        Err(e) => {
            println!("  Cannot build fort in {}: {}", province_name, e);
        }
    }
}

fn print_infrastructure(game: &GameState) {
    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    println!("  INFRASTRUCTURE STATUS:");
    println!();

    for province_id in &player.province_ids {
        let province = match game.get_province(*province_id) {
            Some(p) => p,
            None => continue,
        };

        let mut railroads = 0u32;
        let mut depots = 0u32;
        let mut ports = 0u32;
        let mut total_land = 0u32;

        for coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*coord)
                && tile.terrain().is_land()
            {
                total_land += 1;
                if tile.infrastructure.has_railroad {
                    railroads += 1;
                }
                if tile.infrastructure.has_depot {
                    depots += 1;
                }
                if tile.infrastructure.has_port {
                    ports += 1;
                }
            }
        }

        let connected = infrastructure::is_province_connected(
            &game.hex_map,
            game.get_province(player.capital_province_id)
                .unwrap()
                .capital_tile,
            *province_id,
            &game.provinces,
        );
        let conn_str = if *province_id == player.capital_province_id {
            "(capital)"
        } else if connected {
            "CONNECTED"
        } else {
            "not connected"
        };

        println!(
            "    {}: {} railroads/{} land, {} depot(s), {} port(s) [{}]",
            province.name, railroads, total_land, depots, ports, conn_str
        );
    }
}

// ── Turn report display ───────────────────────────────────────────

fn print_turn_report(game: &GameState, report: &TurnReport) {
    let player_id = game.human_player_nation;

    // ── Newspaper ──────────────────────────────────────────────
    println!();
    println!("  {}", color_bold("THE IMPERIAL TIMES"));
    let date_str = format!(
        "{} Q{}, Turn {}",
        report.year, report.quarter, report.turn.0
    );
    let pad = 42usize.saturating_sub(date_str.len() + 6);
    println!("  \u{2554}{}\u{2557}", "\u{2550}".repeat(42));
    println!("  \u{2551}  {}{}  \u{2551}", date_str, " ".repeat(pad));
    println!("  \u{255a}{}\u{255d}", "\u{2550}".repeat(42));
    for headline in &report.newspaper_headlines {
        println!("    {}", headline);
    }
    println!();

    // ── Compact turn summary ───────────────────────────────────
    let header = format!(
        "Turn {} ({} Q{}) ",
        report.turn.0, report.year, report.quarter
    );
    let line_len = 42usize.saturating_sub(header.len());
    let summary_bar = format!(
        "\u{2500}\u{2500} {}{}",
        color_bold(&header),
        "\u{2500}".repeat(line_len)
    );
    println!("  {}", summary_bar);

    // Economy line: resources gathered
    let mut by_type: std::collections::HashMap<ResourceType, u32> =
        std::collections::HashMap::new();
    for (nid, res, amt) in &report.resource_production {
        if *nid == player_id {
            *by_type.entry(*res).or_insert(0) += amt;
        }
    }
    if !by_type.is_empty() {
        let mut sorted: Vec<_> = by_type.into_iter().collect();
        sorted.sort_by_key(|(r, _)| format!("{:?}", r));
        let resource_parts: Vec<String> = sorted
            .iter()
            .map(|(r, a)| format!("+{} {:?}", a, r))
            .collect();
        let resource_str = resource_parts.join(", ");

        // Food status
        let food_str = if let Some(player) = game.get_nation(player_id) {
            let grain = player.resource_amount(ResourceType::Grain);
            let fruit = player.resource_amount(ResourceType::Fruit);
            let livestock = player.resource_amount(ResourceType::Livestock);
            let total_food = grain + fruit + livestock;
            let food_needed = player.labor.total_workers();
            if total_food >= food_needed {
                color_green(&format!("OK (surplus {})", total_food - food_needed))
            } else {
                color_red(&format!("DEFICIT ({})", food_needed - total_food))
            }
        } else {
            String::new()
        };

        println!(
            "  Economy:  {} | Food: {}",
            color_green(&resource_str),
            food_str
        );
    }

    // Trade line
    let player_trades_bought: Vec<_> = report
        .trade_transactions
        .iter()
        .filter(|txn| txn.buyer == player_id)
        .collect();
    let player_trades_sold: Vec<_> = report
        .trade_transactions
        .iter()
        .filter(|txn| txn.seller == player_id)
        .collect();
    if !player_trades_bought.is_empty() || !player_trades_sold.is_empty() {
        let total_bought: u32 = player_trades_bought.iter().map(|t| t.quantity).sum();
        let total_cost: i64 = player_trades_bought
            .iter()
            .map(|t| t.total_cost.as_dollars())
            .sum();
        let total_earned: i64 = player_trades_sold
            .iter()
            .map(|t| t.total_cost.as_dollars())
            .sum();

        let mut parts = Vec::new();
        if total_bought > 0 {
            parts.push(format!(
                "Bought {} resources for ${}",
                total_bought,
                format_number(total_cost as u32)
            ));
        }
        if total_earned > 0 {
            parts.push(format!("Earned ${}", format_number(total_earned as u32)));
        }
        println!("  Trade:    {}", parts.join(" | "));
    }

    // Subsidy costs line
    let player_subsidies: Vec<_> = report
        .subsidy_costs
        .iter()
        .filter(|(gp, _, _)| *gp == player_id)
        .collect();
    if !player_subsidies.is_empty() {
        let total_subsidy: i64 = player_subsidies
            .iter()
            .map(|(_, _, c)| c.as_dollars())
            .sum();
        println!(
            "  Subsidies: ${} paid to {} Minor Nations",
            format_number(total_subsidy as u32),
            player_subsidies.len()
        );
    }

    // Trade diplomacy line
    let player_diplo: Vec<_> = report
        .trade_diplomacy
        .iter()
        .filter(|(a, _, _)| *a == player_id)
        .collect();
    if !player_diplo.is_empty() {
        let parts: Vec<String> = player_diplo
            .iter()
            .map(|(_, target, imp)| {
                let name = game
                    .get_nation(*target)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| format!("Nation {}", target.0));
                format!("{} +{}", name, imp)
            })
            .collect();
        println!(
            "  Diplomacy: Trade improved relations: {}",
            parts.join(", ")
        );
    }

    // Industry line
    let player_prod: Vec<_> = report
        .production_output
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_prod.is_empty() {
        let parts: Vec<String> = player_prod
            .iter()
            .map(|(_, item, qty)| format!("{} {}", qty, item))
            .collect();
        println!("  Industry: Produced {}", parts.join(", "));
    }

    // Town production line
    let player_town_prod: Vec<_> = report
        .town_production
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_town_prod.is_empty() {
        // Group by province: find producing provinces owned by the player
        let mut town_parts: Vec<String> = Vec::new();
        for (_, item, qty) in &player_town_prod {
            town_parts.push(format!("{} {}", qty, item));
        }
        // Find the producing province names
        let producing_provinces: Vec<String> = game
            .provinces
            .iter()
            .filter(|p| p.owner == player_id && p.can_produce())
            .map(|p| format!("{} ({:?})", p.name, p.settlement_level))
            .collect();
        let prov_info = if producing_provinces.is_empty() {
            String::new()
        } else {
            format!(" from {}", producing_provinces.join(", "))
        };
        println!(
            "  Towns:    Produced {}{}",
            town_parts.join(", "),
            prov_info
        );
    }

    // Unit movement line
    let player_movements: Vec<_> = report
        .unit_movements
        .iter()
        .filter(|(nid, _)| *nid == player_id)
        .collect();
    if !player_movements.is_empty() {
        for (_, desc) in &player_movements {
            println!("  Movement: {}", desc);
        }
    }

    // Military line
    let player_battles: Vec<_> = report
        .battles
        .iter()
        .filter(|b| b.attacker == player_id || b.defender == player_id)
        .collect();
    if player_battles.is_empty() {
        println!("  Military: No battles");
    } else {
        for battle in &player_battles {
            let prov_name = game
                .get_province(battle.province)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            if battle.attacker_won && battle.attacker == player_id {
                println!(
                    "  Military: {}",
                    color_green(&format!("VICTORY at {}", prov_name))
                );
            } else if !battle.attacker_won && battle.defender == player_id {
                println!(
                    "  Military: {}",
                    color_green(&format!("Defended {}", prov_name))
                );
            } else {
                println!(
                    "  Military: {}",
                    color_red(&format!("DEFEAT at {}", prov_name))
                );
            }
        }
    }

    // Naval battles line
    let player_naval_battles: Vec<_> = report
        .naval_battles
        .iter()
        .filter(|b| b.attacker == player_id || b.defender == player_id)
        .collect();
    if !player_naval_battles.is_empty() {
        for nb in &player_naval_battles {
            let enemy_id = if nb.attacker == player_id {
                nb.defender
            } else {
                nb.attacker
            };
            let enemy_name = game
                .get_nation(enemy_id)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let we_won = (nb.attacker_won && nb.attacker == player_id)
                || (!nb.attacker_won && nb.defender == player_id);
            if we_won {
                println!(
                    "  Naval:    {}",
                    color_green(&format!(
                        "NAVAL VICTORY vs {} ({} enemy ships sunk)",
                        enemy_name,
                        if nb.attacker == player_id {
                            nb.defender_ships_lost.len()
                        } else {
                            nb.attacker_ships_lost.len()
                        }
                    ))
                );
            } else {
                println!(
                    "  Naval:    {}",
                    color_red(&format!(
                        "NAVAL DEFEAT vs {} ({} ships lost)",
                        enemy_name,
                        if nb.attacker == player_id {
                            nb.attacker_ships_lost.len()
                        } else {
                            nb.defender_ships_lost.len()
                        }
                    ))
                );
            }
        }
    }

    // Blockade effects (show if any headline mentions BLOCKADE for our nation)
    let blockade_headlines: Vec<_> = report
        .newspaper_headlines
        .iter()
        .filter(|h| h.contains("BLOCKADE"))
        .collect();
    if !blockade_headlines.is_empty() {
        for headline in &blockade_headlines {
            println!("  Blockade: {}", color_yellow(headline));
        }
    }

    // Score line
    if let Some((rank, total)) = score_summary(&report.scores, player_id) {
        println!("  Score:    {} (#{})", format_number(total), rank);
    }

    println!("  {}", "\u{2500}".repeat(44));

    // ── Detailed events (starvation, gold, battles, etc.) ──────
    // Starvation
    for (nid, workers_lost) in &report.starvation {
        if *nid == player_id {
            println!(
                "  {}",
                color_red(&format!(
                    "WARNING: {} workers starved due to food shortage!",
                    workers_lost
                ))
            );
        }
    }

    // Gold income
    for (nid, income) in &report.gold_income {
        if *nid == player_id {
            println!(
                "  {}",
                color_green(&format!("Gold/Gems income: {}", income))
            );
        }
    }

    // Battle details (full report)
    if !report.battles.is_empty() {
        println!();
        for battle in &report.battles {
            let atk_name = game
                .get_nation(battle.attacker)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let def_name = game
                .get_nation(battle.defender)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let prov_name = game
                .get_province(battle.province)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            let result_str = if battle.attacker_won {
                color_green("VICTORY!")
            } else if battle.retreated {
                color_yellow("RETREAT (attacker withdrew)")
            } else {
                color_red("DEFEAT")
            };

            let terrain_str = match battle.terrain {
                Some(t) => {
                    let bonus = domain::military::terrain_defense_bonus(t);
                    if bonus > 0.0 {
                        format!("{:?} (+{:.0}% defense)", t, bonus * 100.0)
                    } else {
                        format!("{:?} (no bonus)", t)
                    }
                }
                None => "Unknown".to_string(),
            };

            let fort_str = if battle.fort_level > 0 {
                if battle.siege_reduced_fort {
                    let full_bonus = domain::military::fort_defense_bonus(battle.fort_level);
                    let reduced_bonus =
                        domain::military::effective_fort_bonus(battle.fort_level, true);
                    format!(
                        "Level {} (+{:.0}% -> +{:.0}% defense, reduced by siege artillery)",
                        battle.fort_level,
                        full_bonus * 100.0,
                        reduced_bonus * 100.0,
                    )
                } else {
                    let bonus = domain::military::fort_defense_bonus(battle.fort_level);
                    format!(
                        "Level {} (+{:.0}% defense)",
                        battle.fort_level,
                        bonus * 100.0
                    )
                }
            } else {
                "None".to_string()
            };

            let atk_casualty_str = if battle.attacker_casualties.is_empty() {
                "None".to_string()
            } else {
                format_casualties(&battle.attacker_casualties)
            };
            let def_casualty_str = if battle.defender_casualties.is_empty() {
                "None".to_string()
            } else {
                format_casualties(&battle.defender_casualties)
            };

            let def_fp_str = if battle.fort_level > 0
                || battle
                    .terrain
                    .map(|t| domain::military::terrain_defense_bonus(t) > 0.0)
                    .unwrap_or(false)
            {
                format!(
                    "{:.1} (incl. terrain/fort bonus)",
                    battle.defender_initial_fp
                )
            } else {
                format!("{:.1}", battle.defender_initial_fp)
            };

            println!("  {}", "=".repeat(42));
            println!("  {}", color_bold(&format!("BATTLE OF {}", prov_name)));
            println!("  {}", "=".repeat(42));
            println!(
                "  Attacker: {} ({} units, FP: {:.1})",
                atk_name, battle.attacker_initial_count, battle.attacker_initial_fp
            );
            println!(
                "  Defender: {} ({} units, FP: {})",
                def_name, battle.defender_initial_count, def_fp_str
            );
            println!("  Terrain: {}", terrain_str);
            println!("  Fort: {}", fort_str);
            println!();
            println!("  Result: {}", result_str);
            println!("  Attacker casualties: {}", atk_casualty_str);
            println!("  Defender casualties: {}", def_casualty_str);
            if battle.retreated {
                println!(
                    "  {}",
                    color_yellow("Attacker retreated! Surviving units took additional damage.")
                );
            }
            if battle.attacker_won {
                println!("  {}", color_green("Province conquered!"));
            }
            // Show medal awards
            if !battle.medal_awards.is_empty() {
                println!();
                println!("  Medal awards:");
                // Group by unit type
                let mut medal_counts: std::collections::BTreeMap<String, Vec<u8>> =
                    std::collections::BTreeMap::new();
                for (ut, count) in &battle.medal_awards {
                    medal_counts
                        .entry(format!("{:?}", ut))
                        .or_default()
                        .push(*count);
                }
                for (unit_name, medals) in &medal_counts {
                    let medal_strs: Vec<String> = medals
                        .iter()
                        .map(|m| {
                            let stars = "*".repeat(*m as usize);
                            format!("[{}]", stars)
                        })
                        .collect();
                    println!("    {} {}", unit_name, medal_strs.join(" "));
                }
            }
            println!("  {}", "=".repeat(42));
        }
    }

    // Civilian completions
    let player_civs: Vec<_> = report
        .civilian_completions
        .iter()
        .filter(|(nid, _)| *nid == player_id)
        .collect();
    if !player_civs.is_empty() {
        println!();
        println!("  Civilian work completed:");
        for (_, desc) in &player_civs {
            println!("    {}", color_green(desc));
        }
    }

    // Transport overflow
    let player_overflow: Vec<_> = report
        .transport_overflow
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_overflow.is_empty() {
        println!();
        println!(
            "  {}",
            color_yellow("Transport overflow (resources left in field):")
        );
        for (_, res, qty) in &player_overflow {
            println!("    {:?}: {}", res, qty);
        }
    }

    // Immigration
    for (nid, count) in &report.immigration {
        if *nid == player_id {
            println!(
                "  {}",
                color_green(&format!(
                    "Immigration: {} new worker{} recruited!",
                    count,
                    if *count == 1 { "" } else { "s" }
                ))
            );
        }
    }

    // Settlement upgrades
    for (prov_id, level) in &report.settlement_upgrades {
        if let Some(prov) = game.get_province(*prov_id) {
            println!(
                "  {}",
                color_green(&format!(
                    "Settlement upgrade: {} is now a {}!",
                    prov.name, level
                ))
            );
        }
    }

    println!();

    // Council vote results
    if let Some(ref vote) = report.council_vote {
        println!("  \u{2554}{}\u{2557}", "\u{2550}".repeat(42));
        println!("  \u{2551}  COUNCIL OF GOVERNORS VOTE            \u{2551}");
        println!("  \u{255a}{}\u{255d}", "\u{2550}".repeat(42));
        for (nid, votes) in &vote.votes {
            let name = game
                .get_nation(*nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let marker = if Some(*nid) == vote.winner {
                " \u{25c4} WINNER"
            } else {
                ""
            };
            println!("    {:<12} {:>3} governors{}", name, votes, marker);
        }
        println!(
            "    Majority needed: {}/{}",
            vote.majority_threshold, vote.total_governors
        );
        println!();
        if let Some(winner_id) = vote.winner
            && winner_id == game.human_player_nation
        {
            println!("  {}", color_green("*** YOU HAVE WON THE GAME! ***"));
        }
    }
}

// ── Overview command ──────────────────────────────────────────────

fn print_overview(game: &GameState) {
    let player_id = game.human_player_nation;
    let player = match game.get_nation(player_id) {
        Some(n) => n,
        None => return,
    };

    let year = game.turn.year();
    let quarter = game.turn.quarter();

    // Count province types
    let total_provinces = player.province_count();
    let homeland_provinces = game
        .provinces
        .iter()
        .filter(|p| p.owner == player_id)
        .count();

    // Population breakdown
    let untrained = player.labor.untrained;
    let trained = player.labor.trained;
    let expert = player.labor.expert;
    let total_workers = player.labor.total_workers();

    // Army
    let army_count = player.army.len();
    let army_fp = {
        let fp = player.total_military_firepower();
        if fp == 0.0 { 0.0 } else { fp }
    };

    // Civilians
    let civilian_count = player.civilians.len();
    let mut civ_types: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for c in &player.civilians {
        *civ_types.entry(format!("{}", c.civilian_type)).or_insert(0) += 1;
    }

    // Freight
    let freight_cars = player.transport.freight_cars;
    let freight_capacity = player.transport.total_capacity();

    // Buildings
    let standard_count = player
        .buildings
        .iter()
        .filter(|b| {
            !matches!(
                b.building_type,
                BuildingType::LumberMill
                    | BuildingType::SteelMill
                    | BuildingType::TextileMill
                    | BuildingType::FurnitureFactory
                    | BuildingType::HardwareFactory
                    | BuildingType::ClothingFactory
            )
        })
        .count();
    let mill_count = player
        .buildings
        .iter()
        .filter(|b| {
            matches!(
                b.building_type,
                BuildingType::LumberMill | BuildingType::SteelMill | BuildingType::TextileMill
            )
        })
        .count();
    let factory_count = player
        .buildings
        .iter()
        .filter(|b| {
            matches!(
                b.building_type,
                BuildingType::FurnitureFactory
                    | BuildingType::HardwareFactory
                    | BuildingType::ClothingFactory
            )
        })
        .count();

    // Tech
    let researched_count = player.researched_techs.len();
    let total_techs = game.game_data.tech_tree.total_tech_count();

    // Score
    let score = calculate_score(player);
    let mut all_scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| (n.id, calculate_score(n).total))
        .collect();
    all_scores.sort_by(|a, b| b.1.cmp(&a.1));
    let rank = all_scores
        .iter()
        .position(|(nid, _)| *nid == player_id)
        .map(|i| i + 1)
        .unwrap_or(0);
    let gp_count = all_scores.len();

    let title = format!(
        "EMPIRE OF {} \u{2014} Year {} Q{}",
        player.name.to_uppercase(),
        year,
        quarter
    );

    println!("  {}", "\u{2550}".repeat(44));
    println!("  {}", color_bold(&title));
    println!("  {}", "\u{2550}".repeat(44));
    println!(
        "  Treasury:     ${}",
        format_number(player.treasury.as_dollars() as u32)
    );
    println!("  Provinces:    {}", homeland_provinces);
    println!(
        "  Population:   {} workers ({} untrained, {} trained, {} expert)",
        total_workers, untrained, trained, expert
    );
    println!("  Army:         {} units (FP: {:.1})", army_count, army_fp);
    if civilian_count > 0 {
        let civ_detail: Vec<String> = civ_types
            .iter()
            .map(|(t, c)| format!("{} {}", c, t))
            .collect();
        println!(
            "  Civilians:    {} ({})",
            civilian_count,
            civ_detail.join(", ")
        );
    } else {
        println!("  Civilians:    0");
    }
    println!(
        "  Freight:      {} cars (capacity: {})",
        freight_cars, freight_capacity
    );
    println!(
        "  Buildings:    {} standard + {} mills + {} factories",
        standard_count, mill_count, factory_count
    );
    println!(
        "  Technologies: {} of {} researched",
        researched_count, total_techs
    );
    println!(
        "  Score:        {} (#{} of {})",
        format_number(score.total),
        rank,
        gp_count
    );
    println!("  {}", "\u{2550}".repeat(44));

    // Suppress unused variable warnings
    let _ = total_provinces;
}

// ── History command ───────────────────────────────────────────────

fn print_history(game: &GameState) {
    if game.history.is_empty() {
        println!("  No major events recorded yet.");
        return;
    }

    println!("  {}", color_bold("HISTORY (last 20 events):"));
    println!();

    let start = if game.history.len() > 20 {
        game.history.len() - 20
    } else {
        0
    };

    for (turn, event) in &game.history[start..] {
        println!(
            "  {} Q{} (Turn {}): {}",
            turn.year(),
            turn.quarter(),
            turn.0,
            event
        );
    }
}

fn print_pending_orders(game: &GameState) {
    let player_id = game.human_player_nation;
    let mut has_orders = false;

    // Pending attacks
    let player_attacks: Vec<_> = game
        .pending_attacks
        .iter()
        .filter(|(nid, _)| *nid == player_id)
        .collect();
    if !player_attacks.is_empty() {
        println!("  PENDING ATTACKS:");
        for (_, province_id) in &player_attacks {
            let prov_name = game
                .get_province(*province_id)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!("    -> Attack on {}", prov_name);
        }
        has_orders = true;
    }

    // Pending unit movements
    let player_moves: Vec<_> = game
        .pending_moves
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_moves.is_empty() {
        println!("  PENDING UNIT MOVEMENTS:");
        for (_, unit_id, dest_id) in &player_moves {
            let unit_desc = game
                .get_nation(player_id)
                .and_then(|n| n.army.iter().find(|u| u.id == *unit_id))
                .map(|u| format!("{:?}", u.unit_type))
                .unwrap_or_else(|| format!("Unit#{}", unit_id.0));
            let dest_name = game
                .get_province(*dest_id)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!("    -> {} moving to {}", unit_desc, dest_name);
        }
        has_orders = true;
    }

    // Working civilians
    if let Some(player) = game.get_nation(player_id) {
        let working: Vec<_> = player.civilians.iter().filter(|c| c.working).collect();
        if !working.is_empty() {
            println!("  WORKING CIVILIANS:");
            for civ in &working {
                let pos_str = civ
                    .position
                    .map(|p| format!("({}, {})", p.q, p.r))
                    .unwrap_or_else(|| "unknown".to_string());
                let remaining = civ.turns_remaining;
                println!(
                    "    {} at {} ({} turn{} remaining)",
                    civ.civilian_type,
                    pos_str,
                    remaining,
                    if remaining == 1 { "" } else { "s" }
                );
            }
            has_orders = true;
        }

        // Buildings under expansion
        let expanding: Vec<_> = player
            .buildings
            .iter()
            .filter(|b| b.is_expanding())
            .collect();
        if !expanding.is_empty() {
            println!("  BUILDINGS UNDER EXPANSION:");
            for bldg in &expanding {
                println!(
                    "    {:?} (expanding, {} turn{} remaining)",
                    bldg.building_type,
                    bldg.expansion_turns_remaining(),
                    if bldg.expansion_turns_remaining() == 1 {
                        ""
                    } else {
                        "s"
                    }
                );
            }
            has_orders = true;
        }
    }

    if !has_orders {
        println!("  No pending orders.");
    }
}

// ── Auto command ──────────────────────────────────────────────────

/// Basic economic automation for the human player during auto-play.
///
/// This gives the human player minimal management so they don't fall behind
/// while fast-forwarding: free tech research and bootstrap mills (same logic
/// the AI gets in `ai_research_tech` / `ai_build_infrastructure`).
fn auto_manage_human(game: &mut GameState) {
    let player_id = game.human_player_nation;

    // Auto-sell excess resources for income (same as AI trade logic)
    if let Some(nation) = game.get_nation_mut(player_id) {
        let tradeable = [
            ResourceType::Timber,
            ResourceType::Coal,
            ResourceType::Iron,
            ResourceType::Cotton,
            ResourceType::Wool,
            ResourceType::Fruit,
            ResourceType::Livestock,
            ResourceType::Oil,
        ];
        for resource in tradeable {
            let amount = nation.resource_amount(resource);
            if amount > 10 {
                let excess = amount - 10;
                let price = domain::economy::trade::base_price(resource);
                if price != Money::ZERO {
                    let revenue = price * excess as i64;
                    nation.remove_resource(resource, excess);
                    nation.treasury += revenue;
                }
            }
        }
    }

    // Auto-research techs: free ones always, affordable ones when treasury allows
    let researched: Vec<_> = match game.get_nation(player_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };
    let treasury = game
        .get_nation(player_id)
        .map(|n| n.treasury)
        .unwrap_or(Money::ZERO);
    let available = game
        .game_data
        .tech_tree
        .available_techs(&researched, game.turn.year());
    // Pick cheapest affordable tech
    let mut affordable_techs: Vec<_> = available
        .iter()
        .filter(|t| treasury.checked_sub(t.cost).is_some())
        .collect();
    affordable_techs.sort_by_key(|t| t.cost.as_dollars());
    if let Some(tech) = affordable_techs.first() {
        let tech_id = tech.id;
        let cost = tech.cost;
        if let Some(nation) = game.get_nation_mut(player_id) {
            nation.treasury -= cost;
            nation.research_tech(tech_id);
        }
    }

    // Auto-build depot on capital and railroads (same as AI)
    {
        let capital_pid = game
            .get_nation(player_id)
            .map(|n| n.capital_province_id)
            .unwrap();
        let capital_tiles: Vec<HexCoord> = game
            .get_province(capital_pid)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();
        // Build depot on first capital tile if affordable
        if let Some(&tile_coord) = capital_tiles.first() {
            let has_depot = game
                .hex_map
                .get_tile(tile_coord)
                .is_some_and(|t| t.infrastructure.has_depot);
            if !has_depot
                && let Ok(cost) = infrastructure::build_depot(&mut game.hex_map, tile_coord)
                && let Some(nation) = game.get_nation_mut(player_id)
                && nation.treasury.checked_sub(cost).is_some()
            {
                nation.treasury -= cost;
            }
        }
        // Build railroads on capital tiles
        for &tile_coord in &capital_tiles {
            let needs_rr = game
                .hex_map
                .get_tile(tile_coord)
                .is_some_and(|t| !t.infrastructure.has_railroad);
            if needs_rr
                && let Ok(cost) = infrastructure::build_railroad(&mut game.hex_map, tile_coord)
                && let Some(nation) = game.get_nation_mut(player_id)
                && let Some(remaining) = nation.treasury.checked_sub(cost)
            {
                nation.treasury = remaining;
            }
        }
    }

    // Auto-build depots and railroads on non-capital provinces (expand infrastructure)
    {
        let province_ids: Vec<ProvinceId> = game
            .get_nation(player_id)
            .map(|n| n.province_ids.clone())
            .unwrap_or_default();
        let capital_pid = game
            .get_nation(player_id)
            .map(|n| n.capital_province_id)
            .unwrap();

        for &pid in &province_ids {
            if pid == capital_pid {
                continue;
            }
            let can_afford = game
                .get_nation(player_id)
                .is_some_and(|n| n.treasury >= Money::dollars(500));
            if !can_afford {
                break;
            }
            let tiles: Vec<HexCoord> = game
                .get_province(pid)
                .map(|p| p.tiles.clone())
                .unwrap_or_default();
            // Build depot on first tile
            if let Some(&tile_coord) = tiles.first() {
                let has_depot = game
                    .hex_map
                    .get_tile(tile_coord)
                    .is_some_and(|t| t.infrastructure.has_depot);
                if !has_depot
                    && let Ok(cost) = infrastructure::build_depot(&mut game.hex_map, tile_coord)
                    && let Some(nation) = game.get_nation_mut(player_id)
                    && let Some(remaining) = nation.treasury.checked_sub(cost)
                {
                    nation.treasury = remaining;
                }
            }
            // Build railroads on tiles
            for &tile_coord in &tiles {
                let can_afford_rr = game
                    .get_nation(player_id)
                    .is_some_and(|n| n.treasury >= Money::dollars(200));
                if !can_afford_rr {
                    break;
                }
                let needs_rr = game
                    .hex_map
                    .get_tile(tile_coord)
                    .is_some_and(|t| !t.infrastructure.has_railroad);
                if needs_rr
                    && let Ok(cost) = infrastructure::build_railroad(&mut game.hex_map, tile_coord)
                    && let Some(nation) = game.get_nation_mut(player_id)
                    && let Some(remaining) = nation.treasury.checked_sub(cost)
                {
                    nation.treasury = remaining;
                }
            }
        }
    }

    // Auto-build first mills (free bootstrap, same as AI)
    let needs_lumber_mill = !game
        .get_nation(player_id)
        .unwrap()
        .has_building(BuildingType::LumberMill);
    let needs_steel_mill = !game
        .get_nation(player_id)
        .unwrap()
        .has_building(BuildingType::SteelMill);
    let needs_textile_mill = !game
        .get_nation(player_id)
        .unwrap()
        .has_building(BuildingType::TextileMill);

    if let Some(nation) = game.get_nation_mut(player_id) {
        if needs_lumber_mill {
            nation
                .buildings
                .push(Building::new(BuildingType::LumberMill, 2));
        }
        if needs_steel_mill {
            nation
                .buildings
                .push(Building::new(BuildingType::SteelMill, 2));
        }
        if needs_textile_mill {
            nation
                .buildings
                .push(Building::new(BuildingType::TextileMill, 2));
        }
    }

    // Auto-build factories if the corresponding mill exists and we have materials
    let nation_ref = game.get_nation(player_id).unwrap();
    let has_lumber_mill = nation_ref.has_building(BuildingType::LumberMill);
    let has_steel_mill = nation_ref.has_building(BuildingType::SteelMill);
    let has_textile_mill = nation_ref.has_building(BuildingType::TextileMill);
    let has_furniture_factory = nation_ref.has_building(BuildingType::FurnitureFactory);
    let has_hardware_factory = nation_ref.has_building(BuildingType::HardwareFactory);
    let has_clothing_factory = nation_ref.has_building(BuildingType::ClothingFactory);
    let lumber_available = nation_ref.material_amount(MaterialType::Lumber);
    let steel_available = nation_ref.material_amount(MaterialType::Steel);

    let needs_furniture = has_lumber_mill && !has_furniture_factory;
    let needs_hardware = has_steel_mill && !has_hardware_factory;
    let needs_clothing = has_textile_mill && !has_clothing_factory;

    if let Some(nation) = game.get_nation_mut(player_id) {
        let mut lumber_left = lumber_available;
        let mut steel_left = steel_available;

        if needs_furniture && lumber_left >= 2 && steel_left >= 2 {
            nation
                .buildings
                .push(Building::new(BuildingType::FurnitureFactory, 1));
            nation.consume_material(MaterialType::Lumber, 1);
            nation.consume_material(MaterialType::Steel, 1);
            lumber_left -= 1;
            steel_left -= 1;
        }
        if needs_hardware && lumber_left >= 2 && steel_left >= 2 {
            nation
                .buildings
                .push(Building::new(BuildingType::HardwareFactory, 1));
            nation.consume_material(MaterialType::Lumber, 1);
            nation.consume_material(MaterialType::Steel, 1);
            lumber_left -= 1;
            steel_left -= 1;
        }
        if needs_clothing && lumber_left >= 2 && steel_left >= 2 {
            nation
                .buildings
                .push(Building::new(BuildingType::ClothingFactory, 1));
            nation.consume_material(MaterialType::Lumber, 1);
            nation.consume_material(MaterialType::Steel, 1);
            lumber_left -= 1;
            steel_left -= 1;
        }

        // Auto-build freight cars: target province_count.max(5), up to 2 per turn
        let target_cars = (nation.province_count() as u32).max(5);
        if nation.transport.freight_cars < target_cars {
            let cars_to_build = (target_cars - nation.transport.freight_cars).min(2);
            let affordable = cars_to_build.min(lumber_left).min(steel_left);
            if affordable > 0 {
                nation.consume_material(MaterialType::Lumber, affordable);
                nation.consume_material(MaterialType::Steel, affordable);
                nation.transport.build_freight_cars(affordable);
            }
        }
    }
}

fn cmd_auto(game: &mut GameState, turns: u32) {
    println!("  Auto-playing {} turns...", turns);

    let mut game_ended = false;

    for i in 1..=turns {
        if game.is_game_over() {
            println!("  Game already ended at turn {}.", game.turn.0);
            game_ended = true;
            break;
        }
        auto_manage_human(game);
        process_turn(game);

        if i % 10 == 0 || i == turns || game.is_game_over() {
            println!(
                "  ...turn {} ({} Q{})",
                i,
                game.turn.year(),
                game.turn.quarter()
            );
        }

        if game.is_game_over() {
            game_ended = true;
            break;
        }
    }

    if game_ended && game.is_game_over() {
        // Record high scores
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

        println!();
        println!("  ══════════════════════════════════════");
        println!("  The game has ended!");
        println!("  ══════════════════════════════════════");
        println!();

        // Show final summary: scores, winner, and human ranking
        print_auto_game_end_summary(game);
        return;
    }

    let player = game.get_nation(game.human_player_nation).unwrap();
    let score = calculate_score(player);
    let mut all_scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| (n.id, calculate_score(n).total))
        .collect();
    all_scores.sort_by(|a, b| b.1.cmp(&a.1));
    let rank = all_scores
        .iter()
        .position(|(nid, _)| *nid == game.human_player_nation)
        .map(|i| i + 1)
        .unwrap_or(0);

    println!();
    println!(
        "  Done. Now at {} Q{}, Treasury: ${}, Score: {} (#{})",
        game.turn.year(),
        game.turn.quarter(),
        format_number(player.treasury.as_dollars() as u32),
        format_number(score.total),
        rank
    );
}

/// Game-over summary for auto-play (no TurnReport needed).
fn print_auto_game_end_summary(game: &GameState) {
    println!("  ╔════════════════════════════════════════╗");
    println!("  ║        FINAL GAME SUMMARY              ║");
    println!("  ╚════════════════════════════════════════╝");
    println!();

    // Final scores for all Great Powers
    println!("    {:<12} {:>8} {:>8}", "Nation", "Score", "Provinces");
    println!("    {}", "-".repeat(32));

    let mut scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| {
            let s = calculate_score(n);
            (n.id, n.name.clone(), s.total, n.province_count())
        })
        .collect();
    scores.sort_by(|a, b| b.2.cmp(&a.2));

    for (id, name, total, prov_count) in &scores {
        let marker = if *id == game.human_player_nation {
            " <-- YOU"
        } else {
            ""
        };
        println!(
            "    {:<12} {:>8} {:>8}{}",
            name,
            format_number(*total),
            prov_count,
            marker
        );
    }
    println!();

    // Winner announcement
    let winner = scores.first();
    if let Some((wid, winner_name, _, _)) = winner {
        if *wid == game.human_player_nation {
            println!(
                "  *** CONGRATULATIONS! {} (YOU) wins the game! ***",
                winner_name
            );
        } else {
            println!("  {} wins the game.", winner_name);
        }
    }

    // Human player ranking
    let human_rank = scores
        .iter()
        .position(|(id, _, _, _)| *id == game.human_player_nation)
        .map(|i| i + 1)
        .unwrap_or(0);
    let total_gp = scores.len();
    println!(
        "  Your ranking: #{} of {} Great Powers",
        human_rank, total_gp
    );
    println!();

    // Show high scores
    if !game.high_scores.is_empty() {
        println!("  High Scores:");
        for (i, (name, score, date)) in game.high_scores.iter().enumerate() {
            println!(
                "    {}. {} - {} ({})",
                i + 1,
                name,
                format_number(*score),
                date
            );
        }
    }
}

fn render_map(hex_map: &HexMap, nations: &[Nation]) {
    let reset = "\x1b[0m";
    let sea_color = "\x1b[34m";

    let mut province_nation: std::collections::HashMap<ProvinceId, &Nation> =
        std::collections::HashMap::new();
    for nation in nations {
        for pid in &nation.province_ids {
            province_nation.insert(*pid, nation);
        }
    }

    for r in 0..hex_map.height() {
        if r % 2 == 1 {
            print!("   ");
        } else {
            print!("  ");
        }

        for q in 0..hex_map.width() {
            let coord = HexCoord::new(q, r);
            match hex_map.get_tile(coord) {
                Some(tile) => {
                    let ch = terrain_char(tile.terrain());
                    if tile.terrain() == TerrainType::Sea {
                        print!("{sea_color}~{reset} ");
                    } else if let Some(pid) = tile.province_id {
                        if let Some(nation) = province_nation.get(&pid) {
                            let color = nation_color_code(nation);
                            if tile.is_capital {
                                print!("{color}★{reset} ");
                            } else {
                                print!("{color}{ch}{reset} ");
                            }
                        } else {
                            print!("{ch} ");
                        }
                    } else {
                        print!("{ch} ");
                    }
                }
                None => print!("  "),
            }
        }
        println!();
    }
}

/// Format a list of casualty unit types into a readable string.
/// Groups identical types: e.g., "2 Militia, 1 Regulars destroyed"
#[allow(dead_code)]
fn format_casualties(casualties: &[ArmyUnitType]) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for ut in casualties {
        *counts.entry(format!("{:?}", ut)).or_insert(0) += 1;
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(name, count)| format!("{} {}", count, name))
        .collect();
    format!("{} destroyed", parts.join(", "))
}

/// Move a unit from its current province to another owned province.
///
/// Usage: move <unit_index> <province_name>
/// - Unit must belong to the player
/// - Target province must be owned by the player
/// - Militia units cannot move
fn cmd_move_unit(game: &mut GameState, args: &str) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: move <unit_index> <province_name>");
        println!("  Example: move 0 France City");
        println!("  Use 'army' to see unit indices.");
        return;
    }

    let index: usize = match parts[0].parse() {
        Ok(i) => i,
        Err(_) => {
            println!(
                "  Invalid unit index: '{}'. Use 'army' to see indices.",
                parts[0]
            );
            return;
        }
    };
    let province_name = parts[1].trim();

    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

    if index >= player.army.len() {
        println!(
            "  Invalid index {}. You have {} army units.",
            index,
            player.army.len()
        );
        return;
    }

    let unit_type = player.army[index].unit_type;
    let unit_id = player.army[index].id;

    // Militia units cannot move
    if !unit_type.can_move() {
        println!("  {:?} units cannot move (garrison only).", unit_type);
        return;
    }

    // Find target province by partial name match (any province, not just owned)
    let lower_name = province_name.to_lowercase();
    let matching_provinces: Vec<_> = game
        .provinces
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&lower_name))
        .collect();

    let target_province = match matching_provinces.len() {
        0 => {
            println!("  No province matches '{}'.", province_name);
            return;
        }
        1 => matching_provinces[0],
        _ => {
            println!(
                "  Multiple provinces match '{}'. Be more specific.",
                province_name
            );
            return;
        }
    };

    let target_province_id = target_province.id;
    let target_name = target_province.name.clone();
    let target_owner = target_province.owner;

    // Check the unit is not already in the target province
    if player.army[index].position == target_province_id {
        println!("  Unit is already in {}.", target_name);
        return;
    }

    if target_owner == player_id {
        // Friendly province: move immediately
        let player = game.get_nation_mut(player_id).unwrap();
        player.army[index].position = target_province_id;
        println!("  Moved {:?} to {}.", unit_type, target_name);
    } else {
        // Check if at war with the province owner
        let at_war = game
            .diplomacy
            .get_relation(player_id, target_owner)
            .is_some_and(|r| r.at_war);
        if at_war {
            // Queue as a pending move (will become an attack at turn resolution)
            game.pending_moves
                .push((player_id, unit_id, target_province_id));
            let owner_name = game
                .get_nation(target_owner)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            println!(
                "  {} is owned by {} (enemy). Attack queued on {} for end of turn.",
                target_name, owner_name, target_name
            );
            println!(
                "  {} pending attack(s) queued. End turn to resolve.",
                game.pending_attacks.len() + game.pending_moves.len()
            );
        } else {
            let owner_name = game
                .get_nation(target_owner)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            println!(
                "  Cannot move to {} -- owned by {} and you are not at war.",
                target_name, owner_name
            );
            println!("  Declare war first with 'war {}'.", owner_name);
        }
    }
}
