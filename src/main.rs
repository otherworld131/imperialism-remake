#![deny(warnings, clippy::all)]

use std::io::{self, Write};
use std::path::PathBuf;

use ::infrastructure::persistence;
use domain::economy::buildings::{Building, BuildingType};
use domain::game_state::{GameState, new_game};
use domain::hex::HexCoord;
use domain::map::infrastructure;
use domain::map::{HexMap, UnitId};
use domain::military::units::{ArmyUnit, ArmyUnitType};
use domain::nation::Nation;
use domain::turn::{TurnReport, calculate_score, process_turn};
use domain::types::*;

use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique UnitIds when building units via CLI.
static NEXT_UNIT_ID: AtomicU32 = AtomicU32::new(2_000_000);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map_key = args.get(1).map(|s| s.as_str()).unwrap_or("imperialism");
    let nation_index: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    println!("╔══════════════════════════════════════════════╗");
    println!("║         IMPERIALISM REMAKE v0.1.0            ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    let mut game = new_game(map_key, Difficulty::Normal, nation_index);

    // Show initial map
    println!("  Map key: \"{}\"", map_key);
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
                println!();
                println!("  ╔════════════════════════════════════════╗");
                println!("  ║  THE IMPERIAL TIMES                   ║");
                println!(
                    "  ║  {} Q{}, Turn {}{}║",
                    report.year,
                    report.quarter,
                    report.turn.0,
                    " ".repeat(
                        25 - format!(
                            "{} Q{}, Turn {}",
                            report.year, report.quarter, report.turn.0
                        )
                        .len()
                    )
                );
                println!("  ╚════════════════════════════════════════╝");
                for headline in &report.newspaper_headlines {
                    println!("    {}", headline);
                }
                println!();

                // Show resource production summary for player
                let player_id = game.human_player_nation;
                let player_production: Vec<_> = report
                    .resource_production
                    .iter()
                    .filter(|(nid, _, _)| *nid == player_id)
                    .collect();
                if !player_production.is_empty() {
                    println!("  Resources gathered this turn:");
                    let mut by_type: std::collections::HashMap<ResourceType, u32> =
                        std::collections::HashMap::new();
                    for (_, res, amt) in &player_production {
                        *by_type.entry(*res).or_insert(0) += amt;
                    }
                    let mut sorted: Vec<_> = by_type.into_iter().collect();
                    sorted.sort_by_key(|(r, _)| format!("{:?}", r));
                    for (res, amt) in &sorted {
                        println!("    {:?}: {}", res, amt);
                    }
                }

                // Show production output
                let player_prod: Vec<_> = report
                    .production_output
                    .iter()
                    .filter(|(nid, _, _)| *nid == player_id)
                    .collect();
                if !player_prod.is_empty() {
                    println!();
                    println!("  Production this turn:");
                    for (_, item, qty) in &player_prod {
                        println!("    {}: {}", item, qty);
                    }
                }

                // Show food consumed
                for (nid, amt) in &report.food_consumed {
                    if *nid == player_id {
                        println!("  Food consumed: {} grain", amt);
                    }
                }

                // Show trade transactions
                let player_trades: Vec<_> = report
                    .trade_transactions
                    .iter()
                    .filter(|txn| txn.buyer == player_id)
                    .collect();
                if !player_trades.is_empty() {
                    println!();
                    println!("  Trade this turn:");
                    for txn in &player_trades {
                        let seller_name = game
                            .get_nation(txn.seller)
                            .map(|n| n.name.as_str())
                            .unwrap_or("Unknown");
                        println!(
                            "    Bought {} {:?} from {} for {}",
                            txn.quantity, txn.resource, seller_name, txn.total_cost
                        );
                    }
                }

                // Show gold income
                for (nid, income) in &report.gold_income {
                    if *nid == player_id {
                        println!("  Gold/Gems income: {}", income);
                    }
                }

                // Show battle results
                if !report.battles.is_empty() {
                    println!();
                    println!("  BATTLE REPORTS:");
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
                            "VICTORY"
                        } else {
                            "DEFEAT"
                        };
                        println!(
                            "    {} vs {} at {} -- {}",
                            atk_name, def_name, prov_name, result_str
                        );
                        if !battle.attacker_casualties.is_empty() {
                            println!(
                                "      Attacker losses: {} units",
                                battle.attacker_casualties.len()
                            );
                        }
                        if !battle.defender_casualties.is_empty() {
                            println!(
                                "      Defender losses: {} units",
                                battle.defender_casualties.len()
                            );
                        }
                        if battle.attacker_won && battle.attacker == player_id {
                            println!("      Province {} conquered!", prov_name);
                        }
                    }
                }
                // Show score summary
                if let Some((rank, total)) = score_summary(&report.scores, game.human_player_nation)
                {
                    let gp_count = report.scores.len();
                    println!(
                        "  Score: {} (#{} of {} Great Powers)",
                        format_number(total),
                        rank,
                        gp_count
                    );
                }
                println!();

                // Show council vote results
                if let Some(ref vote) = report.council_vote {
                    println!("  ╔════════════════════════════════════════╗");
                    println!("  ║  COUNCIL OF GOVERNORS VOTE            ║");
                    println!("  ╚════════════════════════════════════════╝");
                    for (nid, votes) in &vote.votes {
                        let name = game
                            .get_nation(*nid)
                            .map(|n| n.name.as_str())
                            .unwrap_or("Unknown");
                        let marker = if Some(*nid) == vote.winner {
                            " ◄ WINNER"
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
                        println!("  *** YOU HAVE WON THE GAME! ***");
                    }
                }

                if game.is_game_over() {
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
            "b" | "buildings" => {
                println!();
                print_buildings(&game);
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
            "turn10" => {
                for _ in 0..10 {
                    process_turn(&mut game);
                }
                println!(
                    "  Advanced 10 turns. Now: {} ({})",
                    game.turn,
                    game.turn.year()
                );
                print_status(&game);
            }
            "turn100" => {
                for _ in 0..100 {
                    process_turn(&mut game);
                }
                println!(
                    "  Advanced 100 turns. Now: {} ({})",
                    game.turn,
                    game.turn.year()
                );
                print_status(&game);
            }
            "save" => {
                save_current_game(&game);
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
            "infrastructure" | "infra" => {
                println!();
                print_infrastructure(&game);
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
            _ => {
                println!("  Unknown command. Type 'help' for available commands.");
            }
        }
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

    // Cost: 1 lumber + 1 steel per new capacity unit; expand by 1
    let expand_amount: u32 = 1;
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
        "  Expanding {:?} by {} capacity (consumed {} lumber, {} steel). Will be ready in 2 turns.",
        bt, expand_amount, lumber_needed, steel_needed
    );
}

fn recruit_worker(game: &mut GameState) {
    let player_id = game.human_player_nation;
    let player = game.get_nation(player_id).unwrap();

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

    println!(
        "  Recruited 1 untrained worker (now: {} untrained, {} trained, {} expert).",
        player.labor.untrained, player.labor.trained, player.labor.expert
    );
}

fn train_worker(game: &mut GameState) {
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

/// Return the player's rank (1-based) and total score from the turn report scores.
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
            " ◄ YOU"
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

fn print_tech(game: &GameState) {
    let player = game.get_nation(game.human_player_nation).unwrap();
    let year = game.turn.year();

    // Show already researched technologies
    println!("  RESEARCHED TECHNOLOGIES:");
    if player.researched_techs.is_empty() {
        println!("    (none)");
    } else {
        for tech_id in &player.researched_techs {
            if let Some(tech) = game.tech_tree.get(*tech_id) {
                println!("    [x] {}", tech.name);
            }
        }
    }
    println!();

    // Show available technologies
    let available = game
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

            println!("  Researched: {}!", tech_name);
            println!("  Cost: {} (treasury now: {})", tech_cost, player.treasury);
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

    let offers = trade::generate_minor_nation_offers(&game.nations, &game.provinces, &game.hex_map);

    if offers.is_empty() {
        println!("  No trade offerings available from Minor Nations.");
        return;
    }

    // Group offers by seller
    let mut by_seller: std::collections::BTreeMap<String, Vec<&trade::TradeOffer>> =
        std::collections::BTreeMap::new();
    for offer in &offers {
        let seller_name = game
            .get_nation(offer.seller)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("Nation {}", offer.seller.0));
        by_seller.entry(seller_name).or_default().push(offer);
    }

    println!("  MINOR NATION TRADE OFFERINGS:");
    for (name, nation_offers) in &by_seller {
        println!("    {}:", name);
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
    println!("  (Trade is auto-resolved at the start of each turn)");
}

fn print_help() {
    println!("  COMMANDS:");
    println!("    [Enter] / turn    — End turn (gather resources, advance time)");
    println!("    status            — Show current game status");
    println!("    warehouse         — Show your resource warehouse");
    println!("    buildings         — Show your buildings and workers");
    println!("    tech              — Show technology tree status");
    println!("    research <name>   — Research a technology by name");
    println!("    build <building>  — Build a new mill or factory");
    println!("                        (lumbermill, steelmill, textilemill,");
    println!("                         furniturefactory, hardwarefactory, clothingfactory)");
    println!("    expand <building> — Expand an existing building's capacity");
    println!("    recruit           — Recruit an untrained worker (costs 1 canned food,");
    println!("                        1 clothing, 1 furniture)");
    println!("    train             — Train an untrained worker to trained");
    println!("    build railroad    — Build a railroad on the first un-railroaded tile");
    println!("                        in your capital province");
    println!("    build depot       — Build a depot on your capital tile ($2,000)");
    println!(
        "    build port        — Build a port on a coastal tile in your capital province ($3,000)"
    );
    println!("    infra             — Show infrastructure status (railroads, depots, ports)");
    println!("    build car         — Build a freight car (costs 2 labor, 1 lumber, 1 steel)");
    println!("    transport         — Show your transport system (freight cars, capacity)");
    println!("    build unit <type> — Build a military unit");
    println!("                        (regulars $500, grenadiers $1000,");
    println!("                         cuirassiers $500, light artillery $2000)");
    println!("    military / army   — Show your army units and their stats");
    println!("    attack <nation>   — Order an attack on a nation you are at war with");
    println!("    diplomacy         — Show diplomatic relations with all nations");
    println!("    consulate <name>  — Build a trade consulate with a Minor Nation ($500)");
    println!("    embassy <name>    — Build an embassy with a Minor Nation ($5,000)");
    println!("    info <name>       — Show detailed info about any nation");
    println!("    war <name>        — Declare war on a nation");
    println!("    peace <name>      — Propose peace with a nation you are at war with");
    println!("    map               — Show the world map");
    println!("    provinces         — Show your provinces");
    println!("    nations           — Show all nations");
    println!("    trade             — Show Minor Nation trade offerings");
    println!("    score             — Show scores for all Great Powers");
    println!("    save              — Save the current game");
    println!("    load <filename>   — Load a saved game");
    println!("    turn10            — Advance 10 turns at once");
    println!("    turn100           — Advance 100 turns at once");
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

    let filename = format!("save_{}_Q{}.json", game.turn.year(), game.turn.quarter());
    let path = dir.join(&filename);

    match persistence::save_game(game, &path) {
        Ok(()) => {
            println!("  Game saved to: {}", path.display());
        }
        Err(e) => {
            println!("  Failed to save: {}", e);
        }
    }
}

fn load_saved_game(filename: &str) -> Result<GameState, String> {
    let dir = saves_dir();
    let path = dir.join(filename);
    persistence::load_game(&path)
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
    println!("  MINOR NATIONS:");
    for mn in game.minor_nations() {
        let status = match game.diplomacy.get_relation(player_id, mn.id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
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
        println!("    {:<12} {}", mn.name, status);
    }
}

fn cmd_consulate(game: &mut GameState, query: &str) {
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
                "  Trade consulate established with {}! (cost: {}, treasury now: {})",
                target_name, cost, player.treasury
            );
        }
        Err(e) => {
            println!("  Cannot build consulate: {}", e);
        }
    }
}

fn cmd_embassy(game: &mut GameState, query: &str) {
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
                "  Embassy established with {}! (cost: {}, treasury now: {})",
                target_name, cost, player.treasury
            );
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
    println!();
    println!("  ╔════════════════════════════════════════╗");
    println!("  ║  DECLARATION OF WAR                    ║");
    println!("  ╚════════════════════════════════════════╝");
    println!("  Your Excellency has declared WAR upon {}!", target_name);
    println!("  May Providence favor our cause.");
    println!();
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
    println!("  Peace has been established with {}.", target_name);
    println!("  The cannons fall silent.");
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
    let (labor, lumber, steel) =
        domain::economy::transport::TransportSystem::build_freight_car_cost();
    println!(
        "  Use 'build car' to build a freight car (cost: {} labor, {} lumber, {} steel).",
        labor, lumber, steel
    );
}

fn build_freight_car(game: &mut GameState) {
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
        for unit in &player.army {
            let province_name = game
                .get_province(unit.position)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!(
                "    {:?} (HP: {}, Medals: {}, FP: {:.1}) at {}",
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
