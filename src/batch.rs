use std::collections::{BTreeMap, HashMap, HashSet};

use domain::economy::buildings::{Building, BuildingType};
use domain::game_state::{GameState, new_game_with_seed};
use domain::hex::HexCoord;
use domain::map::infrastructure;
use domain::turn::{calculate_score, process_turn};
use domain::types::*;

// ── Batch mode data structures ───────────────────────────────────

#[derive(serde::Serialize)]
pub(crate) struct BatchReport {
    num_games: u32,
    difficulty: String,
    games: Vec<GameReport>,
    aggregate: AggregateReport,
}

#[derive(serde::Serialize)]
pub(crate) struct GameReport {
    seed: String,
    personalities: BTreeMap<String, String>,
    snapshots: BTreeMap<u32, BTreeMap<String, NationSnapshot>>,
    final_scores: BTreeMap<String, u32>,
    wars_declared: BTreeMap<String, u32>,
    winner: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct NationSnapshot {
    treasury: i64,
    provinces: usize,
    army_size: usize,
    worker_count: u32,
    mills: usize,
    factories: usize,
    total_mill_capacity: u32,
    total_factory_capacity: u32,
    other_buildings: usize,
    warships: usize,
    warships_built: u32,
    warships_lost: u32,
    merchant_ships: usize,
    freight_cars: u32,
    tech_count: usize,
    depots: u32,
    railroads: u32,
    forts: u32,
    alliances: usize,
    naps: usize,
    active_wars: usize,
    provinces_gained: i32,
    // Material stockpiles for naval debug (fabric+lumber+arms needed to build a frigate;
    // steel can be converted to arms so it's included too)
    fabric: u32,
    lumber: u32,
    arms: u32,
    steel: u32,
}

#[derive(serde::Serialize)]
pub(crate) struct AggregateReport {
    by_personality: BTreeMap<String, PersonalityStats>,
}

#[derive(serde::Serialize)]
pub(crate) struct PersonalityStats {
    games_played: u32,
    avg_final_score: f64,
    stddev: f64,
    win_rate: f64,
}

// ── Snapshot & helpers ───────────────────────────────────────────

fn take_snapshot(
    game: &GameState,
    starting_provinces: &HashMap<NationId, usize>,
) -> BTreeMap<String, NationSnapshot> {
    let mut snapshots = BTreeMap::new();
    for nation in game.great_powers() {
        let mut depots = 0u32;
        let mut railroads = 0u32;
        let mut forts = 0u32;
        // Only count depots that are actually producing yield (connected to
        // the national rail/port network, or sitting on a country-capital
        // tile which is an implicit hub). Orphan depots inherited from
        // conquered capitals without a rail connection are *not* counted.
        let connected = domain::turn::connected_provinces(game, nation.id);
        for &pid in &nation.province_ids {
            if let Some(province) = game.get_province(pid) {
                for &coord in &province.tiles {
                    if let Some(tile) = game.hex_map.get_tile(coord) {
                        if tile.infrastructure.has_depot {
                            let counts = pid == nation.capital_province_id
                                || tile.is_country_capital
                                || connected.contains(&pid);
                            if counts {
                                depots += 1;
                            }
                        }
                        if tile.infrastructure.has_railroad {
                            railroads += 1;
                        }
                        if tile.infrastructure.has_fort {
                            forts += 1;
                        }
                    }
                }
            }
        }

        let mill_buildings: Vec<&Building> = nation
            .buildings
            .iter()
            .filter(|b| {
                matches!(
                    b.building_type,
                    BuildingType::LumberMill | BuildingType::SteelMill | BuildingType::TextileMill
                )
            })
            .collect();
        let factory_buildings: Vec<&Building> = nation
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
            .collect();
        let mills = mill_buildings.len();
        let total_mill_capacity: u32 = mill_buildings.iter().map(|b| b.effective_capacity()).sum();
        let factories = factory_buildings.len();
        let total_factory_capacity: u32 = factory_buildings
            .iter()
            .map(|b| b.effective_capacity())
            .sum();
        let other_buildings = nation.buildings.len() - mills - factories;

        let mut alliances = 0usize;
        let mut naps = 0usize;
        let mut active_wars = 0usize;
        for other in &game.nations {
            if other.id == nation.id {
                continue;
            }
            if game
                .diplomacy
                .has_treaty(nation.id, other.id, domain::events::TreatyType::Alliance)
            {
                alliances += 1;
            }
            if game.diplomacy.has_treaty(
                nation.id,
                other.id,
                domain::events::TreatyType::NonAggressionPact,
            ) {
                naps += 1;
            }
            if let Some(rel) = game.diplomacy.get_relation(nation.id, other.id)
                && rel.at_war
            {
                active_wars += 1;
            }
        }

        snapshots.insert(
            nation.name.clone(),
            NationSnapshot {
                treasury: nation.treasury.as_dollars(),
                provinces: nation.province_count(),
                army_size: nation.army.len(),
                worker_count: nation.labor.total_workers(),
                mills,
                factories,
                total_mill_capacity,
                total_factory_capacity,
                other_buildings,
                warships: nation.warships.len(),
                warships_built: nation.warships_built,
                warships_lost: nation.warships_lost,
                merchant_ships: nation.merchant_fleet.len(),
                freight_cars: nation.transport.freight_cars,
                tech_count: nation.researched_techs.len(),
                depots,
                railroads,
                forts,
                alliances,
                naps,
                active_wars,
                provinces_gained: nation.province_count() as i32
                    - *starting_provinces.get(&nation.id).unwrap_or(&0) as i32,
                fabric: nation.material_amount(domain::types::MaterialType::Fabric),
                lumber: nation.material_amount(domain::types::MaterialType::Lumber),
                arms: nation.material_amount(domain::types::MaterialType::Arms),
                steel: nation.material_amount(domain::types::MaterialType::Steel),
            },
        );
    }
    snapshots
}

fn get_war_pairs(game: &GameState) -> HashSet<(NationId, NationId)> {
    let mut pairs = HashSet::new();
    for a in &game.nations {
        for b in &game.nations {
            if a.id.0 < b.id.0
                && let Some(rel) = game.diplomacy.get_relation(a.id, b.id)
                && rel.at_war
            {
                pairs.insert((a.id, b.id));
            }
        }
    }
    pairs
}

fn compute_aggregate(games: &[GameReport]) -> AggregateReport {
    let personality_types = ["Aggressive", "Diplomatic", "Economic", "Balanced"];
    let mut by_personality = BTreeMap::new();

    for ptype in &personality_types {
        let mut scores: Vec<f64> = Vec::new();
        let mut wins = 0u32;

        for game in games {
            for (nation_name, personality) in &game.personalities {
                if personality == *ptype {
                    if let Some(&score) = game.final_scores.get(nation_name) {
                        scores.push(score as f64);
                    }
                    if game.winner.as_deref() == Some(nation_name.as_str()) {
                        wins += 1;
                    }
                }
            }
        }

        let count = scores.len();
        if count == 0 {
            by_personality.insert(
                ptype.to_string(),
                PersonalityStats {
                    games_played: 0,
                    avg_final_score: 0.0,
                    stddev: 0.0,
                    win_rate: 0.0,
                },
            );
            continue;
        }

        let mean = scores.iter().sum::<f64>() / count as f64;
        let variance = scores.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / count as f64;
        let stddev = variance.sqrt();
        let win_rate = wins as f64 / count as f64;

        by_personality.insert(
            ptype.to_string(),
            PersonalityStats {
                games_played: count as u32,
                avg_final_score: (mean * 10.0).round() / 10.0,
                stddev: (stddev * 10.0).round() / 10.0,
                win_rate: (win_rate * 1000.0).round() / 1000.0,
            },
        );
    }

    AggregateReport { by_personality }
}

// ── Batch run ────────────────────────────────────────────────────

pub(crate) fn run_batch(n: u32) {
    let ai_debug = std::env::args().any(|a| a == "--ai-debug");
    let snapshot_years: &[u32] = &[1815, 1830, 1845, 1860, 1875, 1890, 1915];
    let mut games_data: Vec<GameReport> = Vec::with_capacity(n as usize);

    for game_idx in 0..n {
        let map_key = format!("batch_{}", game_idx);
        let personality_seed = game_idx as u64 * 6_364_136_223_846_793_005 + 1;
        let mut game = new_game_with_seed(&map_key, Difficulty::Normal, 0, personality_seed);
        // Batch mode: promote the human slot to fully AI-managed so every GP
        // develops. Without this the slot-0 nation has no personality and is
        // skipped by `run_ai_turns`, so it never grows its army/infra.
        let human_id = game.human_player_nation;
        if let Some(nation) = game.get_nation_mut(human_id)
            && nation.ai_personality.is_none()
        {
            let extra =
                domain::ai::common::random_personalities(personality_seed ^ 0xDEAD_BEEF, 1)[0];
            nation.ai_personality = Some(extra);
            let count = domain::ai::priority_target_count(&game.game_data.game_config, extra);
            let targets = domain::ai::pick_priority_minor_targets(&game, human_id, count, &[]);
            if let Some(nation) = game.get_nation_mut(human_id) {
                nation.ai_priority_state.priority_minor_targets = targets;
            }
        }
        game.observer_mode = true;
        game.ai_debug = ai_debug;

        // Record personality assignments
        let mut personalities = BTreeMap::new();
        for nation in game.great_powers() {
            if let Some(p) = nation.ai_personality {
                personalities.insert(nation.name.clone(), p.to_string());
            } else {
                personalities.insert(nation.name.clone(), "Human".to_string());
            }
        }

        let starting_provinces: HashMap<NationId, usize> = game
            .great_powers()
            .iter()
            .map(|n| (n.id, n.province_count()))
            .collect();

        let mut snapshots = BTreeMap::new();

        // Track war declarations by diffing diplomacy state
        let mut wars_declared: BTreeMap<String, u32> = BTreeMap::new();

        while !game.is_game_over() {
            let year = game.turn.year();
            let quarter = game.turn.quarter();

            // Snapshot at Q1 of key years
            if snapshot_years.contains(&year) && quarter == 1 && !snapshots.contains_key(&year) {
                snapshots.insert(year, take_snapshot(&game, &starting_provinces));
            }

            // Record current war state before processing
            let current_wars = get_war_pairs(&game);

            if !game.observer_mode {
                auto_manage_human(&mut game);
            }
            let _report = process_turn(&mut game);

            // Detect new wars
            let new_wars = get_war_pairs(&game);
            for pair in &new_wars {
                if !current_wars.contains(pair) {
                    if let Some(n) = game.get_nation(pair.0) {
                        *wars_declared.entry(n.name.clone()).or_insert(0) += 1;
                    }
                    if let Some(n) = game.get_nation(pair.1) {
                        *wars_declared.entry(n.name.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        // Final snapshot
        let final_year = game.turn.year();
        snapshots.insert(final_year, take_snapshot(&game, &starting_provinces));

        // Final scores + winner
        let mut final_scores = BTreeMap::new();
        let mut best_name = None;
        let mut best_score = 0u32;
        for nation in game.great_powers() {
            let score = calculate_score(nation);
            final_scores.insert(nation.name.clone(), score.total);
            if score.total > best_score {
                best_score = score.total;
                best_name = Some(nation.name.clone());
            }
        }

        games_data.push(GameReport {
            seed: map_key,
            personalities,
            snapshots,
            final_scores,
            wars_declared,
            winner: best_name,
        });

        eprintln!(
            "Game {}/{} complete ({})",
            game_idx + 1,
            n,
            game.turn.year()
        );
    }

    // Aggregate
    let aggregate = compute_aggregate(&games_data);

    let report = BatchReport {
        num_games: n,
        difficulty: "Normal".to_string(),
        games: games_data,
        aggregate,
    };

    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("Error serializing batch report: {}", e);
            std::process::exit(2);
        }
    }
}

// ── Auto management ──────────────────────────────────────────────

/// Basic economic automation for the human player during auto-play.
///
/// This gives the human player minimal management so they don't fall behind
/// while fast-forwarding: free tech research and bootstrap mills (same logic
/// the AI gets in `ai_research_tech` / `ai_build_infrastructure`).
pub(crate) fn auto_manage_human(game: &mut GameState) {
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
        // Build railroads first, then depot (depot now requires railroad or capital tile)
        let provinces_snapshot = game.provinces.clone();
        let cfg_snapshot = game.game_data.game_config.clone();
        let researched: Vec<domain::events::TechId> = game
            .get_nation(player_id)
            .map(|n| n.researched_techs.clone())
            .unwrap_or_default();
        for &tile_coord in &capital_tiles {
            let terrain = match game.hex_map.get_tile(tile_coord) {
                Some(t) if !t.infrastructure.has_railroad => t.terrain(),
                _ => continue,
            };
            let rr_cost = match infrastructure::railroad_cost(terrain, &cfg_snapshot) {
                Some(c) => c,
                None => continue,
            };
            // Verify funds BEFORE mutating the map.
            let can_afford = game
                .get_nation(player_id)
                .is_some_and(|n| n.treasury.checked_sub(rr_cost).is_some());
            if !can_afford {
                continue;
            }
            if let Ok(cost) = infrastructure::build_railroad(
                &mut game.hex_map,
                tile_coord,
                player_id,
                &researched,
                &provinces_snapshot,
                &game.game_data,
                &cfg_snapshot,
            ) && let Some(nation) = game.get_nation_mut(player_id)
            {
                nation.treasury -= cost;
            }
        }
        // Build depot on first capital tile if affordable
        if let Some(&tile_coord) = capital_tiles.first() {
            let has_depot = game
                .hex_map
                .get_tile(tile_coord)
                .is_some_and(|t| t.infrastructure.has_depot);
            let depot_cost = Money::dollars(cfg_snapshot.depot_cost);
            let can_afford = game
                .get_nation(player_id)
                .is_some_and(|n| n.treasury >= depot_cost);
            if !has_depot
                && can_afford
                && let Ok(cost) = infrastructure::build_depot(
                    &mut game.hex_map,
                    tile_coord,
                    player_id,
                    &provinces_snapshot,
                    &cfg_snapshot,
                )
                && let Some(nation) = game.get_nation_mut(player_id)
            {
                nation.treasury -= cost;
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
            let cfg_snapshot = game.game_data.game_config.clone();
            // Gate on the cheapest railroad cost (grassland/forest) to decide if
            // we should keep trying for this province. Using the grassland cost
            // as the floor — if we can't afford that, we can't afford any.
            let floor_cost = Money::dollars(cfg_snapshot.railroad_cost_grassland);
            let can_afford = game
                .get_nation(player_id)
                .is_some_and(|n| n.treasury >= floor_cost);
            if !can_afford {
                break;
            }
            let tiles: Vec<HexCoord> = game
                .get_province(pid)
                .map(|p| p.tiles.clone())
                .unwrap_or_default();
            let provinces_snapshot = game.provinces.clone();
            let researched: Vec<domain::events::TechId> = game
                .get_nation(player_id)
                .map(|n| n.researched_techs.clone())
                .unwrap_or_default();
            // Build railroads on tiles first (depot needs a railroad hex now)
            for &tile_coord in &tiles {
                let terrain = match game.hex_map.get_tile(tile_coord) {
                    Some(t) => t.terrain(),
                    None => continue,
                };
                let rr_cost = match infrastructure::railroad_cost(terrain, &cfg_snapshot) {
                    Some(c) => c,
                    None => continue,
                };
                let can_afford_rr = game
                    .get_nation(player_id)
                    .is_some_and(|n| n.treasury >= rr_cost);
                if !can_afford_rr {
                    continue;
                }
                let needs_rr = game
                    .hex_map
                    .get_tile(tile_coord)
                    .is_some_and(|t| !t.infrastructure.has_railroad);
                if needs_rr
                    && let Ok(cost) = infrastructure::build_railroad(
                        &mut game.hex_map,
                        tile_coord,
                        player_id,
                        &researched,
                        &provinces_snapshot,
                        &game.game_data,
                        &cfg_snapshot,
                    )
                    && let Some(nation) = game.get_nation_mut(player_id)
                    && let Some(remaining) = nation.treasury.checked_sub(cost)
                {
                    nation.treasury = remaining;
                }
            }
            // Build depot on first tile
            if let Some(&tile_coord) = tiles.first() {
                let has_depot = game
                    .hex_map
                    .get_tile(tile_coord)
                    .is_some_and(|t| t.infrastructure.has_depot);
                let depot_cost = Money::dollars(cfg_snapshot.depot_cost);
                let depot_affordable = game
                    .get_nation(player_id)
                    .is_some_and(|n| n.treasury >= depot_cost);
                if !has_depot
                    && depot_affordable
                    && let Ok(cost) = infrastructure::build_depot(
                        &mut game.hex_map,
                        tile_coord,
                        player_id,
                        &provinces_snapshot,
                        &cfg_snapshot,
                    )
                    && let Some(nation) = game.get_nation_mut(player_id)
                {
                    nation.treasury -= cost;
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

    // Auto-build factories: first one of each type is free (same bootstrap as mills)
    {
        let nation_ref = game.get_nation(player_id).unwrap();
        let has_lumber_mill = nation_ref.has_building(BuildingType::LumberMill);
        let has_steel_mill = nation_ref.has_building(BuildingType::SteelMill);
        let has_textile_mill = nation_ref.has_building(BuildingType::TextileMill);
        let needs_furniture =
            has_lumber_mill && !nation_ref.has_building(BuildingType::FurnitureFactory);
        let needs_hardware =
            has_steel_mill && !nation_ref.has_building(BuildingType::HardwareFactory);
        let needs_clothing =
            has_textile_mill && !nation_ref.has_building(BuildingType::ClothingFactory);

        if let Some(nation) = game.get_nation_mut(player_id) {
            if needs_furniture {
                nation
                    .buildings
                    .push(Building::new(BuildingType::FurnitureFactory, 1));
            }
            if needs_hardware {
                nation
                    .buildings
                    .push(Building::new(BuildingType::HardwareFactory, 1));
            }
            if needs_clothing {
                nation
                    .buildings
                    .push(Building::new(BuildingType::ClothingFactory, 1));
            }
        }
    }

    // Auto-build freight cars: target province_count.max(5), up to 2 per turn
    if let Some(nation) = game.get_nation_mut(player_id) {
        let target_cars = (nation.province_count() as u32).max(5);
        if nation.transport.freight_cars < target_cars {
            let lumber_avail = nation.material_amount(MaterialType::Lumber);
            let steel_avail = nation.material_amount(MaterialType::Steel);
            let cars_to_build = (target_cars - nation.transport.freight_cars).min(2);
            let affordable = cars_to_build.min(lumber_avail).min(steel_avail);
            if affordable > 0 {
                nation.consume_material(MaterialType::Lumber, affordable);
                nation.consume_material(MaterialType::Steel, affordable);
                nation.transport.build_freight_cars(affordable);
            }
        }
    }
}

// ── Auto command ─────────────────────────────────────────────────

pub(crate) fn cmd_auto(game: &mut GameState, turns: u32) {
    println!("  Auto-playing {} turns...", turns);

    let mut game_ended = false;

    for i in 1..=turns {
        if game.is_game_over() {
            println!("  Game already ended at turn {}.", game.turn.0);
            game_ended = true;
            break;
        }
        if !game.observer_mode {
            auto_manage_human(game);
        }
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
        crate::display::print_game_end_summary(game, None);
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
        player.treasury,
        crate::display::format_number(score.total),
        rank
    );
}
