#![deny(warnings, clippy::all)]

use std::io::{self, Write};

use domain::game_state::new_game;
use domain::hex::HexCoord;
use domain::map::HexMap;
use domain::nation::Nation;
use domain::turn::process_turn;
use domain::types::*;

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

                // Show gold income
                for (nid, income) in &report.gold_income {
                    if *nid == player_id {
                        println!("  Gold/Gems income: {}", income);
                    }
                }
                println!();

                if game.is_game_over() {
                    println!("  ══════════════════════════════════════");
                    println!("  The year is 1915. The game has ended!");
                    println!("  ══════════════════════════════════════");
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
            _ => {
                println!("  Unknown command. Type 'help' for available commands.");
            }
        }
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

            println!(
                "    {} ({} tiles) [{}]",
                province.name,
                province.tile_count(),
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

fn print_help() {
    println!("  COMMANDS:");
    println!("    [Enter] / turn  — End turn (gather resources, advance time)");
    println!("    status          — Show current game status");
    println!("    warehouse       — Show your resource warehouse");
    println!("    map             — Show the world map");
    println!("    provinces       — Show your provinces");
    println!("    nations         — Show all nations");
    println!("    turn10          — Advance 10 turns at once");
    println!("    turn100         — Advance 100 turns at once");
    println!("    help            — Show this help");
    println!("    quit            — Exit the game");
}

fn print_legend() {
    println!("  Legend: F=Farm f=Forest H=Hills M=Mountain ~=Sea .=Plains");
    println!("         P=Plantation R=Range h=HorseRanch O=Orchard");
    println!("         S=Swamp D=Desert T=Tundra s=Scrub  ★=Capital");
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
