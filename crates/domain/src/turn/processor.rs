use crate::ai::run_ai_turns;
use crate::economy::buildings::BuildingType;
use crate::economy::production::{
    ProductionChain, calculate_factory_production, calculate_mill_production,
};
use crate::economy::trade::{self, TradeTransaction};
use crate::events::*;
use crate::game_state::GameState;
use crate::turn::scoring::{CouncilVoteResult, run_council_vote};
use crate::types::*;

/// Result of processing one turn.
#[derive(Debug)]
pub struct TurnReport {
    pub turn: TurnNumber,
    pub year: u32,
    pub quarter: u32,
    pub events: Vec<DomainEvent>,
    pub resource_production: Vec<(NationId, ResourceType, u32)>,
    pub gold_income: Vec<(NationId, Money)>,
    pub maintenance_costs: Vec<(NationId, Money)>,
    pub production_output: Vec<(NationId, String, u32)>,
    pub food_consumed: Vec<(NationId, u32)>,
    pub newspaper_headlines: Vec<String>,
    pub techs_available: Vec<(NationId, Vec<String>)>,
    pub council_vote: Option<CouncilVoteResult>,
    pub trade_transactions: Vec<TradeTransaction>,
}

/// Process one turn of the game.
pub fn process_turn(game: &mut GameState) -> TurnReport {
    let turn = game.turn;
    let mut report = TurnReport {
        turn,
        year: turn.year(),
        quarter: turn.quarter(),
        events: Vec::new(),
        resource_production: Vec::new(),
        gold_income: Vec::new(),
        maintenance_costs: Vec::new(),
        production_output: Vec::new(),
        food_consumed: Vec::new(),
        newspaper_headlines: Vec::new(),
        techs_available: Vec::new(),
        council_vote: None,
        trade_transactions: Vec::new(),
    };

    // 0. AI decisions for computer-controlled Great Powers
    run_ai_turns(game);

    // 1. Resource production: gather yields from all owned tiles
    collect_resources(game, &mut report);

    // 2. Gold/Gems -> money conversion
    convert_monetary_resources(game, &mut report);

    // 3. Run production chains (mills then factories)
    run_production(game, &mut report);

    // 3b. Trade session: Minor Nations sell resources to Great Powers
    resolve_trade_session(game, &mut report);

    // 4. Tick buildings (process expansion timers)
    tick_buildings(game);

    // 5. Food consumption
    food_consumption(game, &mut report);

    // 6. Maintenance costs (placeholder)
    apply_maintenance(game, &mut report);

    // 7. Report available techs
    report_available_techs(game, &mut report);

    // 8. Council of Governors vote (at decade boundaries)
    check_council_vote(game, &mut report);

    // 9. Generate newspaper
    generate_newspaper(game, &mut report);

    // 10. Advance turn
    report
        .events
        .push(DomainEvent::TurnEnded(TurnEnded { turn }));
    game.advance_turn();
    report
        .events
        .push(DomainEvent::TurnStarted(TurnStarted { turn: game.turn }));

    report
}

/// Collect resource yields from all tiles owned by each nation.
///
/// For each nation, iterates through their provinces, looks up tiles in the hex map,
/// calculates yields, and adds resources to the nation's warehouse.
fn collect_resources(game: &mut GameState, report: &mut TurnReport) {
    // Phase 1: collect production data using immutable borrows
    let mut production_data: Vec<(NationId, ResourceType, u32)> = Vec::new();
    for province in &game.provinces {
        for tile_coord in &province.tiles {
            if let Some(tile) = game.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                production_data.push((
                    province.owner,
                    yield_amount.resource,
                    yield_amount.quantity,
                ));
            }
        }
    }

    // Phase 2: apply to nations using mutable borrows
    for (nation_id, resource, amount) in &production_data {
        if let Some(nation) = game.nations.iter_mut().find(|n| n.id == *nation_id) {
            nation.add_resource(*resource, *amount);
        }
    }

    // Record in report
    report.resource_production.extend(production_data);
}

/// Convert monetary resources (Gold, Gems) into treasury money.
///
/// Gold: each unit = $500
/// Gems: each unit = $1,000
fn convert_monetary_resources(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.nations {
        let gold_amount = nation.resource_amount(ResourceType::Gold);
        let gems_amount = nation.resource_amount(ResourceType::Gems);

        let mut income = Money::ZERO;

        if gold_amount > 0 {
            let gold_value = Money::dollars(gold_amount as i64 * 500);
            income += gold_value;
            nation.remove_resource(ResourceType::Gold, gold_amount);
        }

        if gems_amount > 0 {
            let gems_value = Money::dollars(gems_amount as i64 * 1000);
            income += gems_value;
            nation.remove_resource(ResourceType::Gems, gems_amount);
        }

        if income != Money::ZERO {
            nation.treasury += income;
            report.gold_income.push((nation.id, income));
        }
    }
}

/// Run production chains: mills convert resources to materials, factories convert materials to goods.
///
/// Labor is simplified for now: assumes sufficient labor (constraints added later).
fn run_production(game: &mut GameState, report: &mut TurnReport) {
    let nation_ids: Vec<NationId> = game.nations.iter().map(|n| n.id).collect();

    for nation_id in nation_ids {
        let nation = match game.nations.iter().find(|n| n.id == nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Gather current resource inventory as slices
        let resources: Vec<(ResourceType, u32)> =
            nation.warehouse.iter().map(|(r, q)| (*r, *q)).collect();
        let available_labor = u32::MAX; // simplified: assume sufficient labor

        // ── Mills: resources → materials ──

        // Timber chain: LumberMill
        let lumber_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let timber_result = if lumber_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Timber,
                &resources,
                lumber_mill_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Metal chain: SteelMill
        let steel_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::SteelMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let metal_result = if steel_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Metal,
                &resources,
                steel_mill_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Textile chain: TextileMill
        let textile_mill_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::TextileMill)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let textile_result = if textile_mill_cap > 0 {
            Some(calculate_mill_production(
                ProductionChain::Textile,
                &resources,
                textile_mill_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Apply mill results: consume resources, produce materials
        let nation = game.nations.iter_mut().find(|n| n.id == nation_id).unwrap();

        // Collect newly produced materials to feed into factories
        let mut new_materials: Vec<(MaterialType, u32)> = Vec::new();

        for result in [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
        {
            // Consume resources
            for (resource, amount) in &result.resources_consumed {
                if *amount > 0 {
                    nation.remove_resource(*resource, *amount);
                }
            }
            // Produce materials
            for (material, amount) in &result.materials_produced {
                if *amount > 0 {
                    *nation.materials.entry(*material).or_insert(0) += *amount;
                    new_materials.push((*material, *amount));
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", material), *amount));
                }
            }
        }

        // ── Factories: materials → goods ──

        // Build the current materials inventory for factory input
        let materials_inventory: Vec<(MaterialType, u32)> =
            nation.materials.iter().map(|(m, q)| (*m, *q)).collect();

        // Furniture: LumberMill output → FurnitureFactory
        let furniture_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::FurnitureFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let furniture_result = if furniture_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Timber,
                &materials_inventory,
                furniture_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Hardware: SteelMill output → HardwareFactory
        let hardware_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::HardwareFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let hardware_result = if hardware_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Metal,
                &materials_inventory,
                hardware_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Clothing: TextileMill output → ClothingFactory
        let clothing_cap = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::ClothingFactory)
            .map(|b| b.effective_capacity())
            .unwrap_or(0);

        let clothing_result = if clothing_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Textile,
                &materials_inventory,
                clothing_cap,
                available_labor,
            ))
        } else {
            None
        };

        // Apply factory results: consume materials, produce goods
        for result in [&furniture_result, &hardware_result, &clothing_result]
            .into_iter()
            .flatten()
        {
            // Consume materials
            for (material, amount) in &result.materials_consumed {
                if *amount > 0 {
                    let entry = nation.materials.entry(*material).or_insert(0);
                    *entry = entry.saturating_sub(*amount);
                }
            }
            // Produce goods
            for (good, amount) in &result.goods_produced {
                if *amount > 0 {
                    *nation.goods.entry(*good).or_insert(0) += *amount;
                    report
                        .production_output
                        .push((nation_id, format!("{:?}", good), *amount));
                }
            }
        }
    }
}

/// Tick all buildings for all nations, advancing expansion timers.
fn tick_buildings(game: &mut GameState) {
    for nation in &mut game.nations {
        for building in &mut nation.buildings {
            building.tick();
        }
    }
}

/// Consume food for each nation. Placeholder: consume 1 grain per turn if available.
fn food_consumption(game: &mut GameState, report: &mut TurnReport) {
    for nation in &mut game.nations {
        let grain = nation.resource_amount(ResourceType::Grain);
        if grain > 0 {
            nation.remove_resource(ResourceType::Grain, 1);
            report.food_consumed.push((nation.id, 1));
        }
    }
}

/// Resolve a trade session: generate offers from Minor Nations, auto-generate bids
/// for the human player, resolve trades, and apply the resulting transactions.
fn resolve_trade_session(game: &mut GameState, report: &mut TurnReport) {
    // 1. Generate offers from Minor Nations
    let offers = trade::generate_minor_nation_offers(&game.nations, &game.provinces, &game.hex_map);

    if offers.is_empty() {
        return;
    }

    // 2. Auto-generate bids for the human player: buy 1 of each available resource at base price
    let human_id = game.human_player_nation;
    let human_treasury = match game.get_nation(human_id) {
        Some(n) => n.treasury,
        None => return,
    };

    let mut budget = human_treasury;
    let mut bids = Vec::new();

    // Collect unique tradeable resources from offers
    let mut available_resources: Vec<ResourceType> = offers
        .iter()
        .map(|o| o.resource)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    available_resources.sort_by_key(|r| format!("{:?}", r));

    for resource in available_resources {
        let price = trade::base_price(resource);
        if price == Money::ZERO {
            continue;
        }
        if budget.checked_sub(price).is_some() {
            bids.push(trade::TradeBid {
                buyer: human_id,
                resource,
                quantity: 1,
                max_price_per_unit: price,
            });
            budget -= price;
        }
    }

    // 3. Resolve trades
    let transactions = trade::resolve_trades(&offers, &bids);

    // 4. Apply transactions
    for txn in &transactions {
        // Buyer pays money and receives resources
        if let Some(buyer) = game.get_nation_mut(txn.buyer) {
            buyer.treasury -= txn.total_cost;
            buyer.add_resource(txn.resource, txn.quantity);
        }
        // Seller gets money
        if let Some(seller) = game.get_nation_mut(txn.seller) {
            seller.treasury += txn.total_cost;
        }
    }

    // 5. Record in report
    report.trade_transactions = transactions;
}

/// Apply maintenance costs. Placeholder: no army units tracked in GameState yet,
/// so this is a no-op for now.
fn apply_maintenance(_game: &mut GameState, _report: &mut TurnReport) {
    // For now just a placeholder — deduct $25 per army unit per turn from each nation.
    // We don't have army units in GameState yet, so just log it.
}

/// Report which technologies are available for research by the human player.
fn report_available_techs(game: &GameState, report: &mut TurnReport) {
    let nation = match game.get_nation(game.human_player_nation) {
        Some(n) => n,
        None => return,
    };
    let available = game
        .tech_tree
        .available_techs(&nation.researched_techs, game.turn.year());
    let tech_names: Vec<String> = available.iter().map(|t| t.name.clone()).collect();
    if !tech_names.is_empty() {
        report
            .techs_available
            .push((game.human_player_nation, tech_names));
    }
}

/// Generate newspaper headlines for the turn report.
fn generate_newspaper(game: &GameState, report: &mut TurnReport) {
    let year = game.turn.year();
    let quarter = game.turn.quarter();

    report
        .newspaper_headlines
        .push(format!("The Imperial Times - {year} Q{quarter}"));

    if let Some(human_nation) = game.get_nation(game.human_player_nation) {
        report
            .newspaper_headlines
            .push(format!("The {} empire grows stronger", human_nation.name));
    }

    if game.turn.is_decade_election() {
        report
            .newspaper_headlines
            .push("Council of Governors to convene!".to_string());
    }
}

fn check_council_vote(game: &GameState, report: &mut TurnReport) {
    if !game.turn.is_decade_election() {
        return;
    }

    let is_final = game.turn.is_game_end();
    let result = run_council_vote(&game.nations, &game.provinces, is_final);

    if let Some(winner_id) = result.winner {
        if let Some(winner) = game.get_nation(winner_id) {
            report.newspaper_headlines.push(format!(
                "BREAKING: {} wins the Council of Governors with {} of {} votes!",
                winner.name,
                result
                    .votes
                    .iter()
                    .find(|(id, _)| *id == winner_id)
                    .map(|(_, v)| *v)
                    .unwrap_or(0),
                result.total_governors
            ));
        }
    } else {
        report.newspaper_headlines.push(format!(
            "Council of Governors: No nation achieves the required {} vote majority.",
            result.majority_threshold
        ));
    }

    report.council_vote = Some(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};
    use crate::tech::TechTree;

    /// Build a minimal GameState for testing the turn processor.
    fn test_game_state() -> GameState {
        let coord_farm = HexCoord::new(0, 0);
        let coord_forest = HexCoord::new(1, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A farm tile (produces 1 Grain at level 0)
        let farm_tile = Tile::with_province(TerrainType::Farm, ProvinceId(1));
        hex_map.set_tile(coord_farm, farm_tile);

        // A scrub forest tile (produces 1 Timber always)
        let forest_tile = Tile::with_province(TerrainType::ScrubForest, ProvinceId(1));
        hex_map.set_tile(coord_forest, forest_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Homeland".to_string(),
            NationId(1),
            coord_farm,
            vec![coord_farm, coord_forest],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "Testlandia".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(1000);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1],
            nations: vec![nation1],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
        }
    }

    /// Build a game state with a gold mine for testing monetary conversion.
    fn test_game_state_with_gold() -> GameState {
        let coord_gold = HexCoord::new(0, 0);

        let mut hex_map = HexMap::new(10, 10);

        // A mountain tile with gold deposit at improvement level 1 (produces 1 Gold)
        let mut gold_tile = Tile::with_province(TerrainType::Mountain, ProvinceId(1));
        gold_tile.reveal_deposit(ResourceType::Gold);
        gold_tile.set_improvement_level(1);
        hex_map.set_tile(coord_gold, gold_tile);

        let province1 = Province::new(
            ProvinceId(1),
            "Gold Province".to_string(),
            NationId(1),
            coord_gold,
            vec![coord_gold],
            4,
        );

        let mut nation1 = Nation::new(
            NationId(1),
            "GoldNation".to_string(),
            NationColor::Yellow,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation1.treasury = Money::dollars(2000);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1],
            nations: vec![nation1],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
        }
    }

    // ── Turn advancement ──────────────────────────────────────

    #[test]
    fn process_turn_advances_turn_number() {
        let mut game = test_game_state();
        assert_eq!(game.turn, TurnNumber::new(1));

        let report = process_turn(&mut game);

        assert_eq!(report.turn, TurnNumber::new(1)); // report reflects the turn that was processed
        assert_eq!(game.turn, TurnNumber::new(2)); // game has advanced
    }

    // ── Resource collection ───────────────────────────────────

    #[test]
    fn resource_collection_gathers_from_owned_tiles() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        // Should have collected Grain (from Farm) and Timber (from ScrubForest)
        let grain_produced: u32 = report
            .resource_production
            .iter()
            .filter(|(_, r, _)| *r == ResourceType::Grain)
            .map(|(_, _, q)| q)
            .sum();
        let timber_produced: u32 = report
            .resource_production
            .iter()
            .filter(|(_, r, _)| *r == ResourceType::Timber)
            .map(|(_, _, q)| q)
            .sum();

        assert_eq!(grain_produced, 1); // Farm at level 0 = 1 Grain
        assert_eq!(timber_produced, 1); // ScrubForest = 1 Timber

        // Verify the nation's warehouse was updated
        // Note: food_consumption eats 1 grain per turn, so net grain is 0
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 0);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 1);
    }

    // ── Gold conversion ───────────────────────────────────────

    #[test]
    fn gold_converts_to_money() {
        let mut game = test_game_state_with_gold();
        let initial_treasury = game.get_nation(NationId(1)).unwrap().treasury;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // 1 Gold collected => $500 added to treasury
        assert_eq!(nation.treasury, initial_treasury + Money::dollars(500));

        // Gold should have been removed from warehouse
        assert_eq!(nation.resource_amount(ResourceType::Gold), 0);

        // Report should record the income
        assert!(!report.gold_income.is_empty());
        let (_, income) = report.gold_income[0];
        assert_eq!(income, Money::dollars(500));
    }

    #[test]
    fn gems_convert_to_money() {
        let mut game = test_game_state();
        // Manually add gems to the nation's warehouse
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Gems, 3);

        let initial_treasury = game.get_nation(NationId(1)).unwrap().treasury;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();

        // 3 Gems => $3,000
        assert_eq!(nation.treasury, initial_treasury + Money::dollars(3000));
        assert_eq!(nation.resource_amount(ResourceType::Gems), 0);
        assert!(!report.gold_income.is_empty());
    }

    // ── Newspaper generation ──────────────────────────────────

    #[test]
    fn newspaper_is_generated() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        assert!(!report.newspaper_headlines.is_empty());
        assert!(report.newspaper_headlines[0].contains("The Imperial Times"));
        assert!(report.newspaper_headlines[0].contains("1815"));
        assert!(report.newspaper_headlines[0].contains("Q1"));
    }

    #[test]
    fn newspaper_includes_human_nation() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_empire_headline = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("Testlandia"));
        assert!(has_empire_headline);
    }

    #[test]
    fn newspaper_includes_election_headline() {
        let mut game = test_game_state();
        // Set to 1825 Q1 which is a decade election year
        game.turn = TurnNumber::from_year_quarter(1825, 1);

        let report = process_turn(&mut game);

        let has_election = report
            .newspaper_headlines
            .iter()
            .any(|h| h.contains("Council of Governors"));
        assert!(has_election);
    }

    // ── Multiple turns in sequence ────────────────────────────

    #[test]
    fn multiple_turns_can_be_processed() {
        let mut game = test_game_state();

        for expected_turn in 1..=5 {
            assert_eq!(game.turn, TurnNumber::new(expected_turn));
            let report = process_turn(&mut game);
            assert_eq!(report.turn, TurnNumber::new(expected_turn));
        }
        assert_eq!(game.turn, TurnNumber::new(6));

        // After 5 turns, the nation should have accumulated resources
        // Grain: 1 gathered - 1 consumed each turn = 0 net per turn
        // Timber: 1 gathered per turn, not consumed = 5
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 0);
        assert_eq!(nation.resource_amount(ResourceType::Timber), 5);
    }

    // ── Turn events ───────────────────────────────────────────

    #[test]
    fn turn_events_include_ended_and_started() {
        let mut game = test_game_state();

        let report = process_turn(&mut game);

        let has_ended = report.events.iter().any(|e| {
            matches!(e, DomainEvent::TurnEnded(TurnEnded { turn }) if *turn == TurnNumber::new(1))
        });
        let has_started = report.events.iter().any(|e| {
            matches!(e, DomainEvent::TurnStarted(TurnStarted { turn }) if *turn == TurnNumber::new(2))
        });

        assert!(has_ended);
        assert!(has_started);
    }

    // ── Edge case: no tiles produce nothing ───────────────────

    #[test]
    fn empty_map_produces_nothing() {
        let hex_map = HexMap::new(10, 10);
        let province = Province::new(
            ProvinceId(1),
            "Empty".to_string(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)], // tile exists in province but not in hex_map
            4,
        );
        let mut nation = Nation::new(
            NationId(1),
            "EmptyNation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(500);

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
        };

        let report = process_turn(&mut game);

        assert!(report.resource_production.is_empty());
        assert!(report.gold_income.is_empty());

        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.treasury, Money::dollars(500)); // unchanged
    }

    // ── Gold + Gems combined ──────────────────────────────────

    #[test]
    fn gold_and_gems_both_convert() {
        let mut game = test_game_state();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Gold, 2);
        nation.add_resource(ResourceType::Gems, 1);
        let initial = nation.treasury;

        let _report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // 2 Gold = $1,000, 1 Gems = $1,000 => $2,000 total
        assert_eq!(nation.treasury, initial + Money::dollars(2000));
        assert_eq!(nation.resource_amount(ResourceType::Gold), 0);
        assert_eq!(nation.resource_amount(ResourceType::Gems), 0);
    }

    // ── Production pipeline ───────────────────────────────────

    /// Helper: build a game state with a nation that has buildings and resources.
    fn test_game_state_with_production() -> GameState {
        use crate::economy::buildings::{Building, BuildingType};

        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province = Province::new(
            ProvinceId(1),
            "Industrial".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );

        let mut nation = Nation::new(
            NationId(1),
            "FactoryNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        nation.treasury = Money::dollars(5000);

        // Add mills and factories
        nation
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        nation
            .buildings
            .push(Building::new(BuildingType::SteelMill, 2));
        nation
            .buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        nation
            .buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        nation
            .buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        nation
            .buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province],
            nations: vec![nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
        }
    }

    #[test]
    fn lumber_mill_produces_lumber_from_timber() {
        let mut game = test_game_state_with_production();
        // Add timber to warehouse (need 2 per lumber unit)
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Timber, 6);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, 6 timber / 2 per unit = 3, limited by capacity = 2 lumber produced
        // Then FurnitureFactory cap 1 consumes 2 lumber → 1 furniture
        // Net lumber: 2 - 2 = 0
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            0
        );
        // 6 - 4 consumed = 2 timber remaining
        assert_eq!(nation.resource_amount(ResourceType::Timber), 2);
        // Furniture produced by factory
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );

        // Report should show lumber was produced by the mill
        let lumber_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Lumber")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(lumber_output, 2);
    }

    #[test]
    fn steel_mill_produces_steel_from_coal_and_iron() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Coal, 5);
        nation.add_resource(ResourceType::Iron, 3);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, min(5, 3) = 3 limited by capacity = 2 steel produced
        // Then HardwareFactory cap 1 consumes 2 steel → 1 hardware
        // Net steel: 2 - 2 = 0
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0),
            0
        );
        // 5-2=3 coal, 3-2=1 iron remaining
        assert_eq!(nation.resource_amount(ResourceType::Coal), 3);
        assert_eq!(nation.resource_amount(ResourceType::Iron), 1);
        // Hardware produced
        assert_eq!(
            nation.goods.get(&GoodsType::Hardware).copied().unwrap_or(0),
            1
        );

        let steel_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Steel")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(steel_output, 2);
    }

    #[test]
    fn textile_mill_produces_fabric_from_cotton() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Cotton, 4);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill capacity 2, 4 cotton / 2 per unit = 2 fabric produced
        // Then ClothingFactory cap 1 consumes 2 fabric → 1 clothing
        // Net fabric: 2 - 2 = 0
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Fabric)
                .copied()
                .unwrap_or(0),
            0
        );
        assert_eq!(nation.resource_amount(ResourceType::Cotton), 0);
        // Clothing produced
        assert_eq!(
            nation.goods.get(&GoodsType::Clothing).copied().unwrap_or(0),
            1
        );

        let fabric_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Fabric")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(fabric_output, 2);
    }

    #[test]
    fn furniture_factory_produces_from_lumber() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Pre-stock lumber (bypassing mill)
        *nation.materials.entry(MaterialType::Lumber).or_insert(0) = 4;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 4 lumber / 2 per unit = 2, limited by capacity = 1
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );
        // 4 - 2 consumed = 2 lumber remaining
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            2
        );

        let furniture_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Furniture")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(furniture_output, 1);
    }

    #[test]
    fn hardware_factory_produces_from_steel() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        *nation.materials.entry(MaterialType::Steel).or_insert(0) = 4;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 4 steel / 2 = 2, limited by capacity = 1
        assert_eq!(
            nation.goods.get(&GoodsType::Hardware).copied().unwrap_or(0),
            1
        );
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0),
            2
        );

        let hardware_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Hardware")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(hardware_output, 1);
    }

    #[test]
    fn clothing_factory_produces_from_fabric() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        *nation.materials.entry(MaterialType::Fabric).or_insert(0) = 6;

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Factory capacity 1, 6 fabric / 2 = 3, limited by capacity = 1
        assert_eq!(
            nation.goods.get(&GoodsType::Clothing).copied().unwrap_or(0),
            1
        );
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Fabric)
                .copied()
                .unwrap_or(0),
            4
        );

        let clothing_output: u32 = report
            .production_output
            .iter()
            .filter(|(_, name, _)| name == "Clothing")
            .map(|(_, _, q)| *q)
            .sum();
        assert_eq!(clothing_output, 1);
    }

    #[test]
    fn full_timber_chain_mill_then_factory() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        // Add 8 timber: mill produces 2 lumber (cap 2), then factory makes 1 furniture (cap 1)
        nation.add_resource(ResourceType::Timber, 8);

        let report = process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        // Mill: 8 timber, cap 2 → 2 lumber produced, 4 timber consumed, 4 remain
        // Factory: 2 lumber available, cap 1 → 1 furniture, 2 lumber consumed, 0 lumber remain
        assert_eq!(nation.resource_amount(ResourceType::Timber), 4);
        assert_eq!(
            nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0),
            0
        );
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            1
        );

        // Report should have both lumber and furniture entries
        let has_lumber = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Lumber" && *q > 0);
        let has_furniture = report
            .production_output
            .iter()
            .any(|(_, name, q)| name == "Furniture" && *q > 0);
        assert!(has_lumber);
        assert!(has_furniture);
    }

    #[test]
    fn no_production_without_buildings() {
        let mut game = test_game_state(); // no buildings
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Timber, 10);

        let report = process_turn(&mut game);

        // No production output since there are no mills/factories
        let production_for_nation: Vec<_> = report
            .production_output
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .collect();
        assert!(production_for_nation.is_empty());
        // Timber should still be there
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Timber), 11); // 10 + 1 from forest tile
    }

    #[test]
    fn no_production_without_resources() {
        let mut game = test_game_state_with_production();
        // No resources added; nation has buildings but nothing to process

        let report = process_turn(&mut game);

        let production_for_nation: Vec<_> = report
            .production_output
            .iter()
            .filter(|(nid, _, _)| *nid == NationId(1))
            .collect();
        assert!(production_for_nation.is_empty());
    }

    // ── Food consumption ──────────────────────────────────────

    #[test]
    fn food_consumption_eats_one_grain() {
        let mut game = test_game_state();
        game.get_nation_mut(NationId(1))
            .unwrap()
            .add_resource(ResourceType::Grain, 5);

        let report = process_turn(&mut game);

        // Started with 5, gained 1 from farm = 6, consumed 1 = 5
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 5);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 1);
    }

    #[test]
    fn food_consumption_with_no_grain() {
        let mut game = test_game_state_with_production();
        // No grain in warehouse, no farm tiles

        let report = process_turn(&mut game);

        let consumed: u32 = report
            .food_consumed
            .iter()
            .filter(|(nid, _)| *nid == NationId(1))
            .map(|(_, q)| *q)
            .sum();
        assert_eq!(consumed, 0);
    }

    // ── Building tick ─────────────────────────────────────────

    #[test]
    fn tick_buildings_advances_expansion() {
        use crate::economy::buildings::BuildingType;

        let mut game = test_game_state_with_production();
        // Start expanding the lumber mill
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation
            .get_building_mut(BuildingType::LumberMill)
            .unwrap()
            .start_expansion(3);

        // After 1 turn, expansion countdown should go from 2 to 1
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert_eq!(mill.turns_until_upgrade, 1);
        assert_eq!(mill.capacity, 2); // not yet applied

        // After 2nd turn, expansion should complete
        process_turn(&mut game);

        let nation = game.get_nation(NationId(1)).unwrap();
        let mill = nation
            .buildings
            .iter()
            .find(|b| b.building_type == BuildingType::LumberMill)
            .unwrap();
        assert_eq!(mill.turns_until_upgrade, 0);
        assert_eq!(mill.capacity, 5); // 2 + 3
    }

    // ── Production accumulates over multiple turns ────────────

    #[test]
    fn production_accumulates_over_turns() {
        let mut game = test_game_state_with_production();
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.add_resource(ResourceType::Timber, 20);

        // Run 3 turns
        for _ in 0..3 {
            process_turn(&mut game);
        }

        let nation = game.get_nation(NationId(1)).unwrap();
        // Each turn: mill cap 2 → 2 lumber, factory cap 1 → 1 furniture (consumes 2 lumber)
        // Net lumber per turn: 2 - 2 = 0 remaining (factory consumes what mill produces)
        // Net furniture per turn: 1
        // Timber consumed: 4 per turn = 12 total, 20 - 12 = 8 remaining
        assert_eq!(nation.resource_amount(ResourceType::Timber), 8);
        assert_eq!(
            nation
                .goods
                .get(&GoodsType::Furniture)
                .copied()
                .unwrap_or(0),
            3
        );
    }

    // ── Tech reporting ────────────────────────────────────────

    #[test]
    fn turn_report_includes_available_techs() {
        let mut game = test_game_state();
        // At turn 1, year 1815: should have 2 techs available
        let report = process_turn(&mut game);
        assert!(!report.techs_available.is_empty());
        let (nation_id, techs) = &report.techs_available[0];
        assert_eq!(*nation_id, NationId(1));
        assert!(techs.contains(&"High Pressure Steam Engine".to_string()));
        assert!(techs.contains(&"Seed Drill".to_string()));
    }

    #[test]
    fn turn_report_excludes_researched_techs() {
        let mut game = test_game_state();
        // Research both 1815 techs
        let nation = game.get_nation_mut(NationId(1)).unwrap();
        nation.research_tech(crate::events::TechId(1));
        nation.research_tech(crate::events::TechId(2));

        let report = process_turn(&mut game);
        // No techs should be available at 1815 after researching both
        assert!(
            report.techs_available.is_empty(),
            "No techs should be available after researching all 1815 techs"
        );
    }
}
