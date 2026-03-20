use crate::economy::buildings::{Building, BuildingType};
use crate::economy::trade;
use crate::events::TechId;
use crate::game_state::GameState;
use crate::map::UnitId;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique UnitIds for AI-built army units.
static AI_UNIT_ID_COUNTER: AtomicU32 = AtomicU32::new(2_000_000);

/// Generate a unique UnitId for an AI-built unit.
fn next_unit_id() -> UnitId {
    UnitId(AI_UNIT_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Run AI decisions for all non-human Great Powers.
///
/// Returns a list of notable actions taken by AI nations, suitable for
/// inclusion in the newspaper / turn report.
pub fn run_ai_turns(game: &mut GameState) -> Vec<String> {
    let human_id = game.human_player_nation;
    let current_year = game.turn.year();

    // Collect AI nation IDs
    let ai_nation_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.id != human_id && n.is_great_power())
        .map(|n| n.id)
        .collect();

    let mut actions: Vec<String> = Vec::new();

    for nation_id in &ai_nation_ids {
        ai_research_tech(game, *nation_id, current_year, &mut actions);
        ai_build_infrastructure(game, *nation_id);
        ai_recruit_workers(game, *nation_id);
        ai_build_military(game, *nation_id, &mut actions);
        ai_trade(game, *nation_id);
        ai_build_transport(game, *nation_id);
    }

    ai_declare_wars(game, &ai_nation_ids, &mut actions);

    actions
}

/// Pick the cheapest available tech and research it if the nation can afford it.
fn ai_research_tech(
    game: &mut GameState,
    nation_id: NationId,
    current_year: u32,
    actions: &mut Vec<String>,
) {
    // Gather the nation's researched techs
    let researched: Vec<TechId> = match game.get_nation(nation_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };

    // Find available techs and pick the cheapest
    let available = game.tech_tree.available_techs(&researched, current_year);
    let cheapest = available.iter().min_by_key(|t| t.cost.cents());
    let (tech_id, tech_cost, tech_name) = match cheapest {
        Some(tech) => (tech.id, tech.cost, tech.name.clone()),
        None => return,
    };

    // Check if the nation can afford it
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };
    if let Some(remaining) = nation.treasury.checked_sub(tech_cost) {
        nation.treasury = remaining;
        nation.research_tech(tech_id);
        actions.push(format!(
            "Scientists in {} have discovered {}!",
            nation.name, tech_name
        ));
    }
}

/// Build mills and factories when the nation has the required materials.
fn ai_build_infrastructure(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Build mills if the nation doesn't have them and has materials (1 lumber + 1 steel each)
    let mill_types = [
        BuildingType::LumberMill,
        BuildingType::SteelMill,
        BuildingType::TextileMill,
    ];
    for mill_type in mill_types {
        if !nation.has_building(mill_type) {
            let has_lumber = nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0)
                >= 1;
            let has_steel = nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0)
                >= 1;
            if has_lumber && has_steel {
                *nation.materials.entry(MaterialType::Lumber).or_insert(0) -= 1;
                *nation.materials.entry(MaterialType::Steel).or_insert(0) -= 1;
                nation.buildings.push(Building::new(mill_type, 2));
            }
        }
    }

    // Build factories if the nation has the corresponding mill but not the factory
    let mill_factory_pairs = [
        (BuildingType::LumberMill, BuildingType::FurnitureFactory),
        (BuildingType::SteelMill, BuildingType::HardwareFactory),
        (BuildingType::TextileMill, BuildingType::ClothingFactory),
    ];
    for (mill, factory) in mill_factory_pairs {
        if nation.has_building(mill) && !nation.has_building(factory) {
            let has_lumber = nation
                .materials
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0)
                >= 1;
            let has_steel = nation
                .materials
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0)
                >= 1;
            if has_lumber && has_steel {
                *nation.materials.entry(MaterialType::Lumber).or_insert(0) -= 1;
                *nation.materials.entry(MaterialType::Steel).or_insert(0) -= 1;
                nation.buildings.push(Building::new(factory, 1));
            }
        }
    }
}

/// Recruit a worker if the nation has fewer than 5 total and has surplus food.
///
/// AI only recruits if total food (grain + fruit + livestock) exceeds total workers
/// (i.e., there is a surplus to feed the new worker next turn).
/// AI also processes food first if it has a FoodProcessing building and raw food.
fn ai_recruit_workers(game: &mut GameState, nation_id: NationId) {
    // First, process food if possible
    ai_process_food(game, nation_id);

    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let total_workers = nation.labor.total_workers();
    let grain = nation.resource_amount(ResourceType::Grain);
    let fruit = nation.resource_amount(ResourceType::Fruit);
    let livestock = nation.resource_amount(ResourceType::Livestock);
    let total_food = grain + fruit + livestock;

    // Only recruit if workforce is small AND there is surplus food
    if total_workers < 5 && total_food > total_workers {
        // Consume 1 grain (or fruit/livestock) to recruit
        if nation.resource_amount(ResourceType::Grain) > 0 {
            nation.remove_resource(ResourceType::Grain, 1);
        } else if nation.resource_amount(ResourceType::Fruit) > 0 {
            nation.remove_resource(ResourceType::Fruit, 1);
        } else if nation.resource_amount(ResourceType::Livestock) > 0 {
            nation.remove_resource(ResourceType::Livestock, 1);
        }
        nation.labor.recruit_immigrant();
    }
}

/// AI processes food: if the nation has a FoodProcessing building and raw food,
/// convert raw food to canned food (2 raw -> 1 canned).
fn ai_process_food(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let food_processing_cap = nation
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FoodProcessing)
        .map(|b| b.effective_capacity())
        .unwrap_or(0);

    if food_processing_cap == 0 {
        return;
    }

    let grain = nation.resource_amount(ResourceType::Grain);
    let fruit = nation.resource_amount(ResourceType::Fruit);
    let livestock = nation.resource_amount(ResourceType::Livestock);
    let total_raw = grain + fruit + livestock;

    // Only process if we have excess food beyond worker needs
    let workers = nation.labor.total_workers();
    if total_raw <= workers {
        return; // Don't process food we need to eat
    }

    let available_for_processing = total_raw - workers;
    if available_for_processing < 2 {
        return;
    }

    let raw_limited = available_for_processing / 2;
    let units = food_processing_cap.min(raw_limited);

    if units == 0 {
        return;
    }

    // Consume grain first, then fruit, then livestock
    let mut remaining = units * 2;
    let grain_used = grain.min(remaining);
    remaining -= grain_used;
    let fruit_used = fruit.min(remaining);
    remaining -= fruit_used;
    let livestock_used = livestock.min(remaining);
    let _ = remaining - livestock_used;

    if grain_used > 0 {
        nation.remove_resource(ResourceType::Grain, grain_used);
    }
    if fruit_used > 0 {
        nation.remove_resource(ResourceType::Fruit, fruit_used);
    }
    if livestock_used > 0 {
        nation.remove_resource(ResourceType::Livestock, livestock_used);
    }
    nation.add_material(MaterialType::CannedFood, units);
}

/// Build military units when the nation has sufficient treasury.
///
/// - If nation has < 3 army units AND treasury > $2,000, build a Regulars unit ($500)
/// - If nation has < 5 army units AND treasury > $5,000, build a Grenadiers unit ($1,000)
/// - If nation has >= 5 army units AND treasury > $10,000, build Light Artillery ($2,000)
fn ai_build_military(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    let army_count = nation.army.len();
    let treasury = nation.treasury;
    let capital = nation.capital_province_id;
    let nation_name = nation.name.clone();

    if army_count < 3 && treasury > Money::dollars(2000) {
        let cost = Money::dollars(500);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(next_unit_id(), ArmyUnitType::Regulars, nation_id, capital);
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    } else if army_count < 5 && treasury > Money::dollars(5000) {
        let cost = Money::dollars(1000);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(next_unit_id(), ArmyUnitType::Grenadiers, nation_id, capital);
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    } else if army_count >= 5 && treasury > Money::dollars(10000) {
        let cost = Money::dollars(2000);
        nation.treasury -= cost;
        let unit = ArmyUnit::new(
            next_unit_id(),
            ArmyUnitType::LightArtillery,
            nation_id,
            capital,
        );
        nation.army.push(unit);
        actions.push(format!(
            "{} has been expanding its military forces",
            nation_name
        ));
    }
}

/// Every ~20 turns, each AI Great Power considers declaring war on a Minor Nation.
///
/// Uses turn number + nation id as a pseudo-random seed to pick a target.
/// Only declares war if not already at war with that minor.
fn ai_declare_wars(game: &mut GameState, ai_nation_ids: &[NationId], actions: &mut Vec<String>) {
    if !game.turn.0.is_multiple_of(20) {
        return;
    }

    // Collect minor nation IDs and their capitals
    let minor_nations: Vec<(NationId, ProvinceId, String)> = game
        .nations
        .iter()
        .filter(|n| !n.is_great_power())
        .map(|n| (n.id, n.capital_province_id, n.name.clone()))
        .collect();

    if minor_nations.is_empty() {
        return;
    }

    for &ai_id in ai_nation_ids {
        // Pseudo-random index based on turn + nation id
        let seed = (game.turn.0 as usize).wrapping_add(ai_id.0 as usize);
        let target_index = seed % minor_nations.len();
        let (target_id, target_capital, ref target_name) = minor_nations[target_index];

        // Check if already at war with this minor
        let already_at_war = game
            .diplomacy
            .get_relation(ai_id, target_id)
            .map(|r| r.at_war)
            .unwrap_or(false);

        if !already_at_war {
            let attacker_name = game
                .get_nation(ai_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            game.diplomacy.declare_war(ai_id, target_id);
            game.pending_attacks.push((ai_id, target_capital));
            actions.push(format!(
                "{} has declared war on {}!",
                attacker_name, target_name
            ));
        }
    }
}

/// Sell excess tradeable resources on the market for cash.
///
/// For each tradeable resource the AI has more than 10 of, sell the excess
/// at base_price and add proceeds to the treasury.
fn ai_trade(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Check all tradeable resource types for surplus
    let tradeable_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Oil,
    ];

    for resource in tradeable_resources {
        let amount = nation.resource_amount(resource);
        if amount > 10 {
            let excess = amount - 10;
            let price = trade::base_price(resource);
            if price != Money::ZERO {
                let revenue = price * excess as i64;
                nation.remove_resource(resource, excess);
                nation.treasury += revenue;
            }
        }
    }
}

/// Build freight cars if the nation has none and has the required materials.
///
/// Cost per freight car: 1 lumber + 1 steel (labor requirement simplified away).
/// Builds 2 freight cars if possible.
fn ai_build_transport(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    if nation.transport.freight_cars > 0 {
        return;
    }

    // Need 2 lumber + 2 steel for 2 freight cars
    let has_lumber = nation.material_amount(MaterialType::Lumber) >= 2;
    let has_steel = nation.material_amount(MaterialType::Steel) >= 2;

    if has_lumber && has_steel {
        nation.consume_material(MaterialType::Lumber, 2);
        nation.consume_material(MaterialType::Steel, 2);
        nation.transport.build_freight_cars(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diplomacy::DiplomacyState;
    use crate::hex::HexCoord;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};
    use crate::tech::TechTree;

    /// Build a game state with a human nation and one AI great power.
    fn test_game_with_ai() -> GameState {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            4,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(10000);

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2],
            nations: vec![human_nation, ai_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
        }
    }

    /// Build a game state that includes a minor nation for war tests.
    fn test_game_with_ai_and_minor() -> GameState {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            4,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Minor Capital".to_string(),
            NationId(3),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(10000);

        let minor_nation = Nation::new(
            NationId(3),
            "MinorLand".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![human_nation, ai_nation, minor_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
        }
    }

    // ── Tech research ─────────────────────────────────────────

    #[test]
    fn ai_researches_cheapest_available_tech() {
        let mut game = test_game_with_ai();
        // At 1815, two free techs are available (cost $0):
        // "High Pressure Steam Engine" (ID 1) and "Seed Drill" (ID 2)
        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have researched at least one of the two free techs
        assert!(
            ai.has_researched(TechId(1)) || ai.has_researched(TechId(2)),
            "AI should research a free tech"
        );
        // Treasury reduced by $500 for building a Regulars unit (AI has < 3 army, > $2000)
        assert_eq!(ai.treasury, Money::dollars(9500));
    }

    #[test]
    fn ai_does_not_spend_more_than_it_has() {
        let mut game = test_game_with_ai();
        // Pre-research the free techs so only paid techs remain
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.research_tech(TechId(1));
        ai.research_tech(TechId(2));
        // Set treasury to $500 (less than the cheapest paid tech at $1,000)
        ai.treasury = Money::dollars(500);

        // Move to year 1816 so Cotton Gin ($1,000) becomes available
        game.turn = TurnNumber::from_year_quarter(1816, 1);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should NOT have researched Cotton Gin since it can't afford it
        assert!(
            !ai.has_researched(TechId(3)),
            "AI should not research techs it cannot afford"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(500),
            "Treasury should be unchanged"
        );
    }

    // ── Infrastructure building ──────────────────────────────

    #[test]
    fn ai_builds_mill_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give the AI lumber and steel materials
        *ai.materials.entry(MaterialType::Lumber).or_insert(0) = 3;
        *ai.materials.entry(MaterialType::Steel).or_insert(0) = 3;

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have built a LumberMill (first in the loop)
        assert!(
            ai.has_building(BuildingType::LumberMill),
            "AI should build a LumberMill when it has lumber + steel materials"
        );
    }

    #[test]
    fn ai_builds_factory_when_it_has_mill_and_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give the AI all three mills already so it won't spend materials on them
        ai.buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.buildings.push(Building::new(BuildingType::SteelMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        // Give materials for factory construction
        *ai.materials.entry(MaterialType::Lumber).or_insert(0) = 2;
        *ai.materials.entry(MaterialType::Steel).or_insert(0) = 2;

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.has_building(BuildingType::FurnitureFactory),
            "AI should build a FurnitureFactory when it has a LumberMill and materials"
        );
    }

    #[test]
    fn ai_does_not_build_without_materials() {
        let mut game = test_game_with_ai();
        // AI has no materials at all

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            !ai.has_building(BuildingType::LumberMill),
            "AI should not build buildings without materials"
        );
        assert!(
            !ai.has_building(BuildingType::SteelMill),
            "AI should not build buildings without materials"
        );
    }

    // ── Worker recruitment ───────────────────────────────────

    #[test]
    fn ai_recruits_workers_when_workforce_is_small() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.add_resource(ResourceType::Grain, 5);
        // Starts with 0 workers

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.total_workers(),
            1,
            "AI should recruit 1 worker when workforce < 5 and food available"
        );
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            4,
            "AI should consume 1 grain to recruit"
        );
    }

    #[test]
    fn ai_does_not_recruit_when_workforce_at_five() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.labor.untrained = 5;
        ai.add_resource(ResourceType::Grain, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.total_workers(),
            5,
            "AI should not recruit when it already has 5 workers"
        );
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            5,
            "Grain should be unchanged"
        );
    }

    #[test]
    fn ai_does_not_recruit_without_food() {
        let mut game = test_game_with_ai();
        // AI has 0 grain, 0 workers

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.labor.total_workers(),
            0,
            "AI should not recruit without food"
        );
    }

    // ── Human player not affected ────────────────────────────

    #[test]
    fn ai_does_not_touch_human_player() {
        let mut game = test_game_with_ai();
        let human = game.get_nation_mut(NationId(1)).unwrap();
        let original_treasury = human.treasury;
        let original_techs = human.researched_techs.len();

        run_ai_turns(&mut game);

        let human = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            human.treasury, original_treasury,
            "Human player should not be affected by AI turns"
        );
        assert_eq!(
            human.researched_techs.len(),
            original_techs,
            "Human player techs should not change"
        );
    }

    // ── Military building ────────────────────────────────────

    #[test]
    fn ai_builds_regulars_when_army_small_and_can_afford() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(3000);
        // AI starts with 0 army units

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 1, "AI should build 1 Regulars unit");
        assert_eq!(ai.army[0].unit_type, ArmyUnitType::Regulars);
        assert_eq!(ai.army[0].owner, NationId(2));
        assert_eq!(ai.army[0].position, ProvinceId(2)); // capital
        assert_eq!(
            ai.treasury,
            Money::dollars(2500),
            "Treasury should be reduced by $500"
        );
    }

    #[test]
    fn ai_does_not_build_military_when_poor() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000); // < $2,000 threshold

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.army.is_empty(),
            "AI should not build army units when treasury <= $2,000"
        );
    }

    #[test]
    fn ai_builds_grenadiers_when_army_has_3_units() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(6000);
        // Give AI 3 existing army units
        for i in 0..3 {
            ai.army.push(ArmyUnit::new(
                UnitId(100 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 4, "AI should have built a 4th unit");
        assert_eq!(
            ai.army[3].unit_type,
            ArmyUnitType::Grenadiers,
            "4th unit should be Grenadiers"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(5000),
            "Treasury should be reduced by $1,000"
        );
    }

    #[test]
    fn ai_builds_light_artillery_when_army_large() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(12000);
        // Give AI 5 existing army units
        for i in 0..5 {
            ai.army.push(ArmyUnit::new(
                UnitId(200 + i),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ));
        }

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.army.len(), 6, "AI should have built a 6th unit");
        assert_eq!(
            ai.army[5].unit_type,
            ArmyUnitType::LightArtillery,
            "6th unit should be Light Artillery"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(10000),
            "Treasury should be reduced by $2,000"
        );
    }

    #[test]
    fn ai_military_units_have_unique_ids() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(50000);

        // Run multiple turns to build several units
        let mut actions = Vec::new();
        for _ in 0..5 {
            ai_build_military(&mut game, NationId(2), &mut actions);
        }

        let ai = game.get_nation(NationId(2)).unwrap();
        let ids: Vec<UnitId> = ai.army.iter().map(|u| u.id).collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "AI army units must have unique IDs");
            }
        }
    }

    // ── War declaration ──────────────────────────────────────

    #[test]
    fn ai_declares_war_on_turn_20() {
        let mut game = test_game_with_ai_and_minor();
        // Set to turn 20 (divisible by 20)
        game.turn = TurnNumber::new(20);

        run_ai_turns(&mut game);

        // AI should have declared war on the minor nation
        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        assert!(rel.is_some(), "Relation between AI and minor should exist");
        assert!(
            rel.unwrap().at_war,
            "AI should be at war with the minor nation"
        );
        // Should have queued a pending attack on the minor's capital
        assert!(
            game.pending_attacks
                .iter()
                .any(|(attacker, target)| *attacker == NationId(2) && *target == ProvinceId(3)),
            "AI should queue an attack on the minor's capital"
        );
    }

    #[test]
    fn ai_does_not_declare_war_on_non_multiple_of_20() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(15);

        run_ai_turns(&mut game);

        let rel = game.diplomacy.get_relation(NationId(2), NationId(3));
        // Either no relation exists, or it's not at war
        let at_war = rel.map(|r| r.at_war).unwrap_or(false);
        assert!(
            !at_war,
            "AI should not declare war on non-multiple-of-20 turns"
        );
        assert!(
            game.pending_attacks.is_empty(),
            "No attacks should be pending"
        );
    }

    #[test]
    fn ai_does_not_redeclare_war_on_existing_enemy() {
        let mut game = test_game_with_ai_and_minor();
        game.turn = TurnNumber::new(20);

        // Pre-set war
        game.diplomacy.declare_war(NationId(2), NationId(3));

        run_ai_turns(&mut game);

        // Should not have queued a duplicate pending attack
        let attack_count = game
            .pending_attacks
            .iter()
            .filter(|(a, _)| *a == NationId(2))
            .count();
        assert_eq!(
            attack_count, 0,
            "AI should not queue attack if already at war"
        );
    }

    // ── Trade ────────────────────────────────────────────────

    #[test]
    fn ai_sells_excess_resources() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000);
        // Give AI 15 timber (surplus over 10 threshold)
        ai.add_resource(ResourceType::Timber, 15);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have sold 5 timber at $50 each = $250
        assert_eq!(
            ai.resource_amount(ResourceType::Timber),
            10,
            "AI should sell down to 10 timber"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(1250),
            "Treasury should increase by $250 from selling 5 timber at $50"
        );
    }

    #[test]
    fn ai_does_not_sell_resources_below_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000);
        ai.add_resource(ResourceType::Timber, 8); // below threshold of 10

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.resource_amount(ResourceType::Timber),
            8,
            "AI should not sell resources at or below 10"
        );
    }

    #[test]
    fn ai_sells_multiple_excess_resources() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Timber, 15); // 5 excess at $50 = $250
        ai.add_resource(ResourceType::Coal, 20); // 10 excess at $75 = $750

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.resource_amount(ResourceType::Timber), 10);
        assert_eq!(ai.resource_amount(ResourceType::Coal), 10);
        assert_eq!(
            ai.treasury,
            Money::dollars(1000),
            "Treasury should increase by $250 + $750 = $1000"
        );
    }

    #[test]
    fn ai_does_not_sell_non_tradeable_grain() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Grain, 20); // grain is not in the tradeable list

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Grain has base_price $0, and is not in the tradeable_resources list in ai_trade
        // so it should remain untouched. Worker recruitment may consume 1 grain.
        // But with 0 workers and < 5 workers, 1 grain consumed for recruitment.
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            19,
            "Only 1 grain consumed for worker recruitment, none sold"
        );
    }

    // ── Transport building ───────────────────────────────────

    #[test]
    fn ai_builds_freight_cars_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give all buildings so infrastructure doesn't consume materials
        ai.buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.buildings.push(Building::new(BuildingType::SteelMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        ai.buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        ai.buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));
        ai.add_material(MaterialType::Lumber, 5);
        ai.add_material(MaterialType::Steel, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 2,
            "AI should build 2 freight cars"
        );
        // Should have consumed 2 lumber + 2 steel
        assert_eq!(ai.material_amount(MaterialType::Lumber), 3);
        assert_eq!(ai.material_amount(MaterialType::Steel), 3);
    }

    #[test]
    fn ai_does_not_build_freight_cars_without_materials() {
        let mut game = test_game_with_ai();
        // AI has no materials

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 0,
            "AI should not build freight cars without materials"
        );
    }

    #[test]
    fn ai_does_not_build_freight_cars_if_already_has_some() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.transport.build_freight_cars(1); // already has cars
        ai.add_material(MaterialType::Lumber, 5);
        ai.add_material(MaterialType::Steel, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 1,
            "AI should not build more freight cars when it already has some"
        );
        // Materials should be untouched (except if used by infrastructure building)
    }
}
