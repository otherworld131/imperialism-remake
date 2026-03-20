use crate::events::*;
use crate::game_state::GameState;
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
    pub newspaper_headlines: Vec<String>,
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
        newspaper_headlines: Vec::new(),
    };

    // 1. Resource production: gather yields from all owned tiles
    collect_resources(game, &mut report);

    // 2. Gold/Gems -> money conversion
    convert_monetary_resources(game, &mut report);

    // 3. Generate newspaper
    generate_newspaper(game, &mut report);

    // 4. Advance turn
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::HexCoord;
    use crate::map::tile::Tile;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};

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
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 1);
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
        let nation = game.get_nation(NationId(1)).unwrap();
        assert_eq!(nation.resource_amount(ResourceType::Grain), 5);
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
}
