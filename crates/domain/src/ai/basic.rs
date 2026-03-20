use crate::economy::buildings::{Building, BuildingType};
use crate::events::TechId;
use crate::game_state::GameState;
use crate::types::*;

/// Run AI decisions for all non-human Great Powers.
pub fn run_ai_turns(game: &mut GameState) {
    let human_id = game.human_player_nation;
    let current_year = game.turn.year();

    // Collect AI nation IDs
    let ai_nation_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.id != human_id && n.is_great_power())
        .map(|n| n.id)
        .collect();

    for nation_id in ai_nation_ids {
        ai_research_tech(game, nation_id, current_year);
        ai_build_infrastructure(game, nation_id);
        ai_recruit_workers(game, nation_id);
    }
}

/// Pick the cheapest available tech and research it if the nation can afford it.
fn ai_research_tech(game: &mut GameState, nation_id: NationId, current_year: u32) {
    // Gather the nation's researched techs
    let researched: Vec<TechId> = match game.get_nation(nation_id) {
        Some(n) => n.researched_techs.clone(),
        None => return,
    };

    // Find available techs and pick the cheapest
    let available = game.tech_tree.available_techs(&researched, current_year);
    let cheapest = available.iter().min_by_key(|t| t.cost.cents());
    let (tech_id, tech_cost) = match cheapest {
        Some(tech) => (tech.id, tech.cost),
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

/// Recruit a worker if the nation has fewer than 5 total and has food in the warehouse.
fn ai_recruit_workers(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    if nation.labor.total_workers() < 5 && nation.resource_amount(ResourceType::Grain) > 0 {
        nation.remove_resource(ResourceType::Grain, 1);
        nation.labor.recruit_immigrant();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        // Treasury should be unchanged since the techs cost $0
        assert_eq!(ai.treasury, Money::dollars(10000));
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
}
