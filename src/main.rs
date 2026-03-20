#![deny(warnings, clippy::all)]

use std::io::{self, Write};
use std::path::PathBuf;

use domain::economy::buildings::{Building, BuildingType};
use domain::game_state::{GameState, new_game};
use domain::hex::HexCoord;
use domain::map::HexMap;
use domain::military::units::ArmyUnitType;
use domain::nation::Nation;
use domain::turn::{calculate_score, process_turn};
use domain::types::*;
use infrastructure::persistence;

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

    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= cost;

    println!(
        "  Unit built! {:?} (cost: {}, treasury now: {}).",
        unit_type, cost, player.treasury
    );
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
    println!("    build unit <type> — Build a military unit");
    println!("                        (regulars $500, grenadiers $1000,");
    println!("                         cuirassiers $500, light artillery $2000)");
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
