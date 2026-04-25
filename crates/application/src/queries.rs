//! Screen data queries — the "query" side of CQRS.
//! These extract view data from game state for display.

use domain::diplomacy::DiplomaticRelation;
use domain::economy::trade::base_price;
use domain::game_state::GameState;
use domain::types::*;

/// Data for the Map Screen (Screen 1).
pub struct MapScreenData {
    pub turn: String,
    pub nation_name: String,
    pub treasury: String,
    pub province_count: usize,
    pub army_count: usize,
    pub civilian_count: usize,
}

/// Data for the Transport Screen (Screen 2).
pub struct TransportScreenData {
    pub freight_cars: u32,
    pub total_capacity: u32,
    pub total_production: u32,
    pub utilization_percent: u32,
}

/// Data for the Industry Screen (Screen 3).
pub struct IndustryScreenData {
    pub buildings: Vec<(String, u32, bool)>, // name, capacity, expanding
    pub workers: (u32, u32, u32),            // untrained, trained, expert
    pub warehouse_summary: Vec<(String, u32)>, // resource name, amount
}

/// Data for the Trade Screen (Screen 4).
pub struct TradeScreenData {
    pub partners: Vec<TradePartnerData>,
    pub cargo_capacity: u32,
    pub cargo_used: u32,
}

/// Data about a trade partner for the Trade Screen.
pub struct TradePartnerData {
    pub nation_name: String,
    pub nation_id: NationId,
    pub has_consulate: bool,
    pub relationship_score: i32,
    pub available_resources: Vec<(String, u32, String)>, // resource, qty, price
}

/// Data for the Diplomacy Screen (Screen 5).
pub struct DiplomacyScreenData {
    pub standing: i32,
    pub great_power_relations: Vec<(String, NationId, String, i32)>, // name, id, status, score
    pub minor_nation_relations: Vec<(String, NationId, String, i32)>, // name, id, infra, score
    pub council_projection: Vec<(String, u32)>,                      // nation, projected votes
}

// ── Query functions ──────────────────────────────────────────────

/// Extract data for the Map Screen from the current game state.
pub fn get_map_screen(game: &GameState) -> MapScreenData {
    let nation = game
        .get_nation(game.human_player_nation)
        .expect("Human player nation must exist");

    MapScreenData {
        turn: format!("{}", game.turn),
        nation_name: nation.name.clone(),
        treasury: format!("${}", nation.treasury.as_dollars()),
        province_count: nation.province_count(),
        army_count: nation.army.len(),
        civilian_count: nation.civilians.len(),
    }
}

/// Extract data for the Transport Screen from the current game state.
pub fn get_transport_screen(game: &GameState) -> TransportScreenData {
    let nation = game
        .get_nation(game.human_player_nation)
        .expect("Human player nation must exist");

    let freight_cars = nation.transport.freight_cars;
    let total_capacity = nation.transport.total_capacity();

    // Total production is the sum of all raw resources in the warehouse.
    let total_production: u32 = nation.warehouse.values().sum();

    let utilization_percent = if total_capacity > 0 {
        let used = total_production.min(total_capacity);
        (used * 100) / total_capacity
    } else {
        0
    };

    TransportScreenData {
        freight_cars,
        total_capacity,
        total_production,
        utilization_percent,
    }
}

/// Extract data for the Industry Screen from the current game state.
pub fn get_industry_screen(game: &GameState) -> IndustryScreenData {
    let nation = game
        .get_nation(game.human_player_nation)
        .expect("Human player nation must exist");

    let buildings: Vec<(String, u32, bool)> = nation
        .buildings
        .iter()
        .map(|b| {
            (
                format!("{}", b.building_type),
                b.effective_capacity(),
                b.is_expanding(),
            )
        })
        .collect();

    let workers = (
        nation.labor.untrained,
        nation.labor.trained,
        nation.labor.expert,
    );

    // Aggregate all resources, materials, and goods into a single warehouse summary.
    let mut warehouse_summary: Vec<(String, u32)> = Vec::new();

    for (resource, &amount) in &nation.warehouse {
        if amount > 0 {
            warehouse_summary.push((format!("{:?}", resource), amount)); // ResourceType Debug names are user-friendly
        }
    }
    for (material, &amount) in &nation.materials {
        if amount > 0 {
            warehouse_summary.push((format!("{}", material), amount));
        }
    }
    for (goods, &amount) in &nation.goods {
        if amount > 0 {
            warehouse_summary.push((format!("{:?}", goods), amount)); // GoodsType Debug names are user-friendly
        }
    }

    // Sort for deterministic output.
    warehouse_summary.sort_by(|a, b| a.0.cmp(&b.0));

    IndustryScreenData {
        buildings,
        workers,
        warehouse_summary,
    }
}

/// Extract data for the Trade Screen from the current game state.
///
/// Only shows minor nations with which the human player has a consulate.
pub fn get_trade_screen(game: &GameState) -> TradeScreenData {
    let human_id = game.human_player_nation;
    let nation = game
        .get_nation(human_id)
        .expect("Human player nation must exist");

    let cargo_capacity = nation.total_cargo_capacity();
    let cargo_used: u32 = nation
        .trade_history
        .iter()
        .filter(|th| th.turn.0 <= game.turn.0 && game.turn.0.saturating_sub(th.turn.0) <= 1)
        .filter(|th| th.partner != nation.id)
        .map(|th| th.quantity)
        .sum::<u32>()
        .min(cargo_capacity);

    let mut partners = Vec::new();

    for minor in game.minor_nations() {
        let relation = game.diplomacy.get_relation(human_id, minor.id);

        let has_consulate = relation.map(|r| r.has_consulate).unwrap_or(false);

        // Only include nations where the player has a consulate.
        if !has_consulate {
            continue;
        }

        let relationship_score = relation.map(|r| r.score).unwrap_or(0);

        // Available resources: what the minor nation has in its warehouse.
        let available_resources: Vec<(String, u32, String)> = minor
            .warehouse
            .iter()
            .filter(|(_, qty)| **qty > 0)
            .map(|(resource, qty)| {
                let price = base_price(*resource);
                (format!("{:?}", resource), *qty, format!("{}", price))
            })
            .collect();

        partners.push(TradePartnerData {
            nation_name: minor.name.clone(),
            nation_id: minor.id,
            has_consulate,
            relationship_score,
            available_resources,
        });
    }

    TradeScreenData {
        partners,
        cargo_capacity,
        cargo_used,
    }
}

/// Extract data for the Diplomacy Screen from the current game state.
pub fn get_diplomacy_screen(game: &GameState) -> DiplomacyScreenData {
    let human_id = game.human_player_nation;
    let standing = game.diplomacy.get_standing(human_id);

    // Great Power relations
    let great_power_relations: Vec<(String, NationId, String, i32)> = game
        .great_powers()
        .iter()
        .filter(|gp| gp.id != human_id)
        .map(|gp| {
            let relation = game.diplomacy.get_relation(human_id, gp.id);
            let (status, score) = diplomat_status(relation);
            (gp.name.clone(), gp.id, status, score)
        })
        .collect();

    // Minor Nation relations
    let minor_nation_relations: Vec<(String, NationId, String, i32)> = game
        .minor_nations()
        .iter()
        .map(|mn| {
            let relation = game.diplomacy.get_relation(human_id, mn.id);
            let infra = diplomatic_infrastructure(relation);
            let score = relation.map(|r| r.score).unwrap_or(0);
            (mn.name.clone(), mn.id, infra, score)
        })
        .collect();

    // Council projection: provinces per Great Power.
    let council_projection: Vec<(String, u32)> = game
        .great_powers()
        .iter()
        .map(|gp| (gp.name.clone(), gp.province_count() as u32))
        .collect();

    DiplomacyScreenData {
        standing,
        great_power_relations,
        minor_nation_relations,
        council_projection,
    }
}

/// Determine the diplomatic status string for a Great Power relation.
fn diplomat_status(relation: Option<&DiplomaticRelation>) -> (String, i32) {
    match relation {
        Some(rel) => {
            let status = if rel.at_war {
                "At War".to_string()
            } else if rel.has_treaty(domain::events::TreatyType::Alliance) {
                "Allied".to_string()
            } else if rel.has_treaty(domain::events::TreatyType::NonAggressionPact) {
                "Non-Aggression Pact".to_string()
            } else {
                "Neutral".to_string()
            };
            (status, rel.score)
        }
        None => ("No Relations".to_string(), 0),
    }
}

/// Determine the diplomatic infrastructure string for a Minor Nation relation.
fn diplomatic_infrastructure(relation: Option<&DiplomaticRelation>) -> String {
    match relation {
        Some(rel) => {
            if rel.has_embassy {
                "Embassy".to_string()
            } else if rel.has_consulate {
                "Consulate".to_string()
            } else {
                "None".to_string()
            }
        }
        None => "None".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::economy::buildings::BuildingType;
    use domain::game_state::new_game;

    // ── Map Screen ──────────────────────────────────────────────────

    #[test]
    fn map_screen_returns_valid_data_for_new_game() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_map_screen(&game);

        assert_eq!(data.turn, "1815 Q1");
        assert!(!data.nation_name.is_empty());
        assert!(data.treasury.starts_with('$'));
        assert!(data.province_count > 0);
        // Normal difficulty starts with 2 civilians (Farmer + Forester)
        assert_eq!(data.civilian_count, 2);
        // Each Great Power starts with 5 army units
        assert_eq!(data.army_count, 5);
    }

    // ── Transport Screen ────────────────────────────────────────────

    #[test]
    fn transport_screen_returns_valid_data_for_new_game() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_transport_screen(&game);

        // Each Great Power starts with 5 freight cars (from game.lua config)
        assert_eq!(data.freight_cars, 5);
        assert_eq!(data.total_capacity, 5);
    }

    #[test]
    fn transport_utilization_calculation_is_correct() {
        let mut game = new_game("test", Difficulty::Normal, 0);
        let human_id = game.human_player_nation;
        let nation = game.get_nation_mut(human_id).unwrap();

        // Clear any starting warehouse contents to get a clean baseline
        nation.warehouse.clear();

        // Reset freight cars and build exactly 10 for a clean test
        nation.transport.freight_cars = 0;
        nation.transport.build_freight_cars(10);
        nation.add_resource(ResourceType::Timber, 3);
        nation.add_resource(ResourceType::Coal, 2);

        let data = get_transport_screen(&game);

        assert_eq!(data.freight_cars, 10);
        assert_eq!(data.total_capacity, 10);
        assert_eq!(data.total_production, 5); // 3 timber + 2 coal
        // 5 out of 10 capacity = 50%
        assert_eq!(data.utilization_percent, 50);
    }

    #[test]
    fn transport_utilization_capped_at_100() {
        let mut game = new_game("test", Difficulty::Normal, 0);
        let human_id = game.human_player_nation;
        let nation = game.get_nation_mut(human_id).unwrap();

        // More resources than capacity
        nation.transport.build_freight_cars(5);
        nation.add_resource(ResourceType::Timber, 10);

        let data = get_transport_screen(&game);

        // Production exceeds capacity, but utilization is capped at 100%
        assert_eq!(data.utilization_percent, 100);
    }

    // ── Industry Screen ─────────────────────────────────────────────

    #[test]
    fn industry_screen_shows_all_buildings() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_industry_screen(&game);

        let human = game.get_nation(game.human_player_nation).unwrap();

        // The number of buildings in the query output should match the nation
        assert_eq!(data.buildings.len(), human.buildings.len());
        assert!(!data.buildings.is_empty());

        // All fixed buildings should be present on Normal difficulty:
        let building_names: Vec<&str> = data
            .buildings
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect();
        assert!(building_names.contains(&"Armory"));
        assert!(building_names.contains(&"Capitol"));
        assert!(building_names.contains(&"Food Processing"));
        assert!(building_names.contains(&"Railyard"));
        assert!(building_names.contains(&"Shipyard"));
        assert!(building_names.contains(&"Trade School"));
        assert!(building_names.contains(&"University"));
        assert!(building_names.contains(&"Warehouse"));
    }

    #[test]
    fn industry_screen_shows_workers() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_industry_screen(&game);

        let (untrained, trained, expert) = data.workers;
        // Normal difficulty starting labor
        assert_eq!(untrained, 4);
        assert_eq!(trained, 2);
        assert_eq!(expert, 1);
    }

    #[test]
    fn industry_screen_shows_expanding_building() {
        let mut game = new_game("test", Difficulty::Easy, 0);
        let human_id = game.human_player_nation;
        let nation = game.get_nation_mut(human_id).unwrap();

        // Start expanding the LumberMill
        if let Some(mill) = nation.get_building_mut(BuildingType::LumberMill) {
            mill.start_expansion(3);
        }

        let data = get_industry_screen(&game);

        // Find the LumberMill entry and verify it is expanding
        let lumber_mill = data
            .buildings
            .iter()
            .find(|(name, _, _)| name == "Lumber Mill");
        assert!(lumber_mill.is_some());
        let (_, _capacity, is_expanding) = lumber_mill.unwrap();
        assert!(is_expanding);
    }

    // ── Trade Screen ────────────────────────────────────────────────

    #[test]
    fn trade_screen_shows_only_nations_with_consulates() {
        let mut game = new_game("test", Difficulty::Normal, 0);
        let human_id = game.human_player_nation;

        // At start, no consulates exist, so no trade partners
        let data = get_trade_screen(&game);
        assert!(
            data.partners.is_empty(),
            "New game should have no trade partners (no consulates)"
        );

        // Build a consulate with the first minor nation
        let first_minor_id = game.minor_nations()[0].id;
        game.diplomacy
            .build_consulate(human_id, first_minor_id)
            .unwrap();

        // Now query again
        let data = get_trade_screen(&game);
        assert_eq!(
            data.partners.len(),
            1,
            "Should have exactly 1 trade partner after building 1 consulate"
        );
        assert_eq!(data.partners[0].nation_id, first_minor_id);
        assert!(data.partners[0].has_consulate);
    }

    #[test]
    fn trade_screen_shows_cargo_capacity() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_trade_screen(&game);

        // Each Great Power starts with 1 Trader ship
        let nation = game.get_nation(game.human_player_nation).unwrap();
        assert_eq!(data.cargo_capacity, nation.total_cargo_capacity());
    }

    #[test]
    fn trade_screen_excludes_great_powers() {
        let mut game = new_game("test", Difficulty::Normal, 0);
        let human_id = game.human_player_nation;

        // Build consulates with two minor nations
        let minor_ids: Vec<NationId> = game.minor_nations().iter().map(|n| n.id).collect();
        game.diplomacy
            .build_consulate(human_id, minor_ids[0])
            .unwrap();
        game.diplomacy
            .build_consulate(human_id, minor_ids[1])
            .unwrap();

        let data = get_trade_screen(&game);

        // All partners should be minor nations (no great powers)
        for partner in &data.partners {
            let nation = game.get_nation(partner.nation_id).unwrap();
            assert!(
                !nation.is_great_power(),
                "Trade screen should not include Great Powers"
            );
        }
        assert_eq!(data.partners.len(), 2);
    }

    #[test]
    fn trade_screen_cargo_used_counts_current_and_previous_turn_only() {
        use domain::economy::trade::TradeHistoryEntry;

        let mut game = new_game("test", Difficulty::Normal, 0);
        let human_id = game.human_player_nation;
        let partner = NationId(99);
        game.turn = TurnNumber(5);

        let nation = game.get_nation_mut(human_id).unwrap();
        nation.trade_history.clear();

        // Turn 5 (current) — should count
        nation.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber(5),
            partner,
            resource: ResourceType::Timber,
            quantity: 3,
            total_cost: Money::dollars(30),
                bought: true,});
        // Turn 4 (previous) — should count
        nation.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber(4),
            partner,
            resource: ResourceType::Coal,
            quantity: 2,
            total_cost: Money::dollars(20),
                bought: true,});
        // Turn 3 (older) — should NOT count
        nation.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber(3),
            partner,
            resource: ResourceType::Iron,
            quantity: 10,
            total_cost: Money::dollars(100),
                bought: true,});

        let data = get_trade_screen(&game);
        let capacity = game.get_nation(human_id).unwrap().total_cargo_capacity();
        let expected = (3u32 + 2).min(capacity);
        assert_eq!(data.cargo_used, expected);
    }

    // ── Diplomacy Screen ────────────────────────────────────────────

    #[test]
    fn diplomacy_screen_shows_correct_standing() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_diplomacy_screen(&game);

        // Default standing is 100
        assert_eq!(data.standing, 100);
    }

    #[test]
    fn diplomacy_screen_shows_all_other_great_powers() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_diplomacy_screen(&game);

        // 7 Great Powers - 1 (human) = 6 other Great Powers
        assert_eq!(data.great_power_relations.len(), 6);
    }

    #[test]
    fn diplomacy_screen_shows_all_minor_nations() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_diplomacy_screen(&game);

        assert_eq!(data.minor_nation_relations.len(), 16);
    }

    #[test]
    fn diplomacy_screen_shows_council_projection() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_diplomacy_screen(&game);

        // All 7 Great Powers should appear in council projection
        assert_eq!(data.council_projection.len(), 7);

        // Each Great Power should have at least 1 province (their capital)
        for (_, votes) in &data.council_projection {
            assert!(*votes > 0, "Each GP should have at least 1 province");
        }
    }

    #[test]
    fn diplomacy_screen_gp_relations_have_neutral_status() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_diplomacy_screen(&game);

        // At game start, all GP relations should be Neutral (embassies exist but no treaties)
        for (_, _, status, _) in &data.great_power_relations {
            assert_eq!(
                status, "Neutral",
                "All GP relations should be Neutral at game start"
            );
        }
    }

    #[test]
    fn diplomacy_screen_minor_nations_start_with_no_infrastructure() {
        let game = new_game("test", Difficulty::Normal, 0);
        let data = get_diplomacy_screen(&game);

        // No consulates or embassies at start
        for (_, _, infra, _) in &data.minor_nation_relations {
            assert_eq!(
                infra, "None",
                "All minor nation relations should have no infrastructure at game start"
            );
        }
    }

    #[test]
    fn diplomacy_screen_reflects_reduced_standing() {
        let mut game = new_game("test", Difficulty::Normal, 0);
        let human_id = game.human_player_nation;

        // Reduce standing
        game.diplomacy.reduce_standing(human_id, 30);

        let data = get_diplomacy_screen(&game);
        assert_eq!(data.standing, 70);
    }
}
