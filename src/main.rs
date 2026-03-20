#![deny(warnings, clippy::all)]

use domain::game_state::new_game;
use domain::hex::HexCoord;
use domain::map::HexMap;
use domain::nation::Nation;
use domain::types::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map_key = args.get(1).map(|s| s.as_str()).unwrap_or("imperialism");
    let nation_index: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    println!("╔══════════════════════════════════════════════╗");
    println!("║         IMPERIALISM REMAKE v0.1.0            ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();
    println!("  Map key: \"{}\"", map_key);

    let game = new_game(map_key, Difficulty::Normal, nation_index);

    let player_nation = game.get_nation(game.human_player_nation).unwrap();
    println!(
        "  Playing as: {} ({})",
        player_nation.name,
        if player_nation.is_great_power() {
            "Great Power"
        } else {
            "Minor Nation"
        }
    );
    println!("  Turn: {} ({})", game.turn, game.turn.year());
    println!("  Treasury: {}", player_nation.treasury);
    println!("  Difficulty: {:?}", game.difficulty);
    println!();

    // ── Nations summary ──────────────────────────────────────────
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
    println!();

    // ── Map display ──────────────────────────────────────────────
    println!(
        "  MAP ({} x {}):",
        game.hex_map.width(),
        game.hex_map.height()
    );
    println!();
    render_map(&game.hex_map, &game.nations);
    println!();

    // ── Province detail for player nation ────────────────────────
    println!("  YOUR PROVINCES:");
    for province in &game.provinces {
        if province.owner == game.human_player_nation {
            // Count terrain types in province
            let mut terrain_counts = std::collections::HashMap::new();
            for tile_coord in &province.tiles {
                if let Some(tile) = game.hex_map.get_tile(*tile_coord) {
                    *terrain_counts
                        .entry(terrain_char(tile.terrain()))
                        .or_insert(0) += 1;
                }
            }
            let terrain_summary: String = terrain_counts
                .iter()
                .map(|(ch, count)| format!("{}×{}", count, ch))
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
    println!();
    println!("  Legend: F=Farm f=Forest H=Hills M=Mountain ~=Sea .=Plains");
    println!("         P=Plantation R=Range h=HorseRanch O=Orchard");
    println!("         S=Swamp D=Desert T=Tundra s=Scrub");
    println!();
    println!(
        "  Game ready. {} total tiles, {} provinces.",
        game.hex_map.tile_count(),
        game.provinces.len()
    );
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

    // Build province→nation lookup
    let mut province_nation: std::collections::HashMap<ProvinceId, &Nation> =
        std::collections::HashMap::new();
    for nation in nations {
        for pid in &nation.province_ids {
            province_nation.insert(*pid, nation);
        }
    }

    for r in 0..hex_map.height() {
        // Hex offset for odd rows
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
