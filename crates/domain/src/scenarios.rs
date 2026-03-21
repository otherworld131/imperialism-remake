use crate::game_state::GameState;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct ScenarioInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub year: u32,
    pub description: &'static str,
    pub great_powers: Vec<&'static str>,
}

/// List available scenarios.
pub fn list_scenarios() -> Vec<ScenarioInfo> {
    vec![
        ScenarioInfo {
            id: "1815",
            name: "Congress of Vienna",
            year: 1815,
            description: "Post-Napoleonic Europe. The Great Powers redraw the map.",
            great_powers: vec![
                "Britain",
                "France",
                "Prussia",
                "Austria",
                "Russia",
                "Spain",
                "Netherlands",
            ],
        },
        ScenarioInfo {
            id: "1820",
            name: "Concert of Europe",
            year: 1820,
            description: "The Concert of Europe maintains a fragile balance of power.",
            great_powers: vec![
                "Britain",
                "France",
                "Prussia",
                "Austria",
                "Russia",
                "Spain",
                "Ottoman Empire",
            ],
        },
        ScenarioInfo {
            id: "1848",
            name: "Year of Revolutions",
            year: 1848,
            description: "Revolution sweeps across Europe. Empires tremble.",
            great_powers: vec![
                "Britain",
                "France",
                "Prussia",
                "Austria",
                "Russia",
                "Sardinia",
                "Ottoman Empire",
            ],
        },
        ScenarioInfo {
            id: "1882",
            name: "Scramble for Africa",
            year: 1882,
            description: "The Great Powers compete for colonial dominance.",
            great_powers: vec![
                "Britain",
                "France",
                "Germany",
                "Italy",
                "Russia",
                "Ottoman Empire",
                "Austria-Hungary",
            ],
        },
    ]
}

/// Create a game from a historical scenario.
/// For now, uses the standard map generator but sets the start year and nation names.
pub fn new_scenario_game(
    scenario_id: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
) -> Result<GameState, String> {
    let scenario = list_scenarios()
        .into_iter()
        .find(|s| s.id == scenario_id)
        .ok_or_else(|| format!("Unknown scenario: {}", scenario_id))?;

    let mut game = crate::game_state::new_game(
        &format!("scenario_{}", scenario_id),
        difficulty,
        human_nation_index,
    );

    // Set the start year
    game.turn = TurnNumber::from_year_quarter(scenario.year, 1);

    // Rename Great Powers to historical names
    let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    for (i, gp_id) in gp_ids.iter().enumerate() {
        if i < scenario.great_powers.len()
            && let Some(nation) = game.get_nation_mut(*gp_id)
        {
            nation.name = scenario.great_powers[i].to_string();
        }
    }

    // For 1820, pre-research the first 2 free techs (High Pressure Steam Engine, Seed Drill)
    if scenario.year >= 1820 && scenario.year < 1848 {
        let free_techs = vec![crate::events::TechId(1), crate::events::TechId(2)];
        for nation in &mut game.nations {
            if nation.is_great_power() {
                for tech_id in &free_techs {
                    if !nation.has_researched(*tech_id) {
                        nation.research_tech(*tech_id);
                    }
                }
            }
        }
    }

    // For 1848+, pre-research some early techs for all nations
    if scenario.year >= 1848 {
        let early_techs: Vec<crate::events::TechId> = game
            .tech_tree
            .all_techs()
            .iter()
            .filter(|t| t.earliest_year <= 1840)
            .map(|t| t.id)
            .collect();
        for nation in &mut game.nations {
            if nation.is_great_power() {
                for tech_id in &early_techs {
                    if !nation.has_researched(*tech_id) {
                        nation.research_tech(*tech_id);
                    }
                }
            }
        }
    }

    Ok(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_list_returns_4_scenarios() {
        let scenarios = list_scenarios();
        assert_eq!(scenarios.len(), 4);
    }

    #[test]
    fn scenario_ids_are_unique() {
        let scenarios = list_scenarios();
        let ids: Vec<&str> = scenarios.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["1815", "1820", "1848", "1882"]);
    }

    #[test]
    fn scenario_game_starts_at_correct_year_1815() {
        let game = new_scenario_game("1815", Difficulty::Normal, 0).unwrap();
        assert_eq!(game.turn.year(), 1815);
        assert_eq!(game.turn.quarter(), 1);
    }

    #[test]
    fn scenario_game_starts_at_correct_year_1848() {
        let game = new_scenario_game("1848", Difficulty::Normal, 0).unwrap();
        assert_eq!(game.turn.year(), 1848);
        assert_eq!(game.turn.quarter(), 1);
    }

    #[test]
    fn scenario_game_starts_at_correct_year_1882() {
        let game = new_scenario_game("1882", Difficulty::Normal, 0).unwrap();
        assert_eq!(game.turn.year(), 1882);
        assert_eq!(game.turn.quarter(), 1);
    }

    #[test]
    fn scenario_game_has_correct_nation_names_1815() {
        let game = new_scenario_game("1815", Difficulty::Normal, 0).unwrap();
        let gp_names: Vec<String> = game.great_powers().iter().map(|n| n.name.clone()).collect();
        assert_eq!(
            gp_names,
            vec![
                "Britain",
                "France",
                "Prussia",
                "Austria",
                "Russia",
                "Spain",
                "Netherlands"
            ]
        );
    }

    #[test]
    fn scenario_game_has_correct_nation_names_1848() {
        let game = new_scenario_game("1848", Difficulty::Normal, 0).unwrap();
        let gp_names: Vec<String> = game.great_powers().iter().map(|n| n.name.clone()).collect();
        assert_eq!(
            gp_names,
            vec![
                "Britain",
                "France",
                "Prussia",
                "Austria",
                "Russia",
                "Sardinia",
                "Ottoman Empire"
            ]
        );
    }

    #[test]
    fn scenario_game_has_correct_nation_names_1882() {
        let game = new_scenario_game("1882", Difficulty::Normal, 0).unwrap();
        let gp_names: Vec<String> = game.great_powers().iter().map(|n| n.name.clone()).collect();
        assert_eq!(
            gp_names,
            vec![
                "Britain",
                "France",
                "Germany",
                "Italy",
                "Russia",
                "Ottoman Empire",
                "Austria-Hungary"
            ]
        );
    }

    #[test]
    fn scenario_1848_pre_researches_early_techs() {
        let game = new_scenario_game("1848", Difficulty::Normal, 0).unwrap();
        // Techs with earliest_year <= 1840 should be pre-researched for Great Powers
        // TechId(1) = "High Pressure Steam Engine" (1815)
        // TechId(2) = "Seed Drill" (1815)
        // TechId(3) = "Cotton Gin" (1816)
        // TechId(4) = "Iron Railroad Bridge" (1821)
        // TechId(5) = "Feed Grasses" (1821)
        // TechId(6) = "Square-Set Timbering" (1821)
        // TechId(7) = "Streamlined Hulls" (1821)
        // TechId(8) = "Spinning Jenny" (1826)
        // TechId(9) = "Paddlewheels" (1826)
        // TechId(10) = "Steel Plows" (1831)
        // TechId(11) = "Bessemer Converter" (1836)
        // TechId(12) = "Compound Steam Engine" (1836)
        for nation in game.great_powers() {
            assert!(
                nation.has_researched(crate::events::TechId(1)),
                "{} should have researched High Pressure Steam Engine",
                nation.name
            );
            assert!(
                nation.has_researched(crate::events::TechId(2)),
                "{} should have researched Seed Drill",
                nation.name
            );
            assert!(
                nation.has_researched(crate::events::TechId(3)),
                "{} should have researched Cotton Gin",
                nation.name
            );
        }
    }

    #[test]
    fn scenario_game_starts_at_correct_year_1820() {
        let game = new_scenario_game("1820", Difficulty::Normal, 0).unwrap();
        assert_eq!(game.turn.year(), 1820);
        assert_eq!(game.turn.quarter(), 1);
    }

    #[test]
    fn scenario_game_has_correct_nation_names_1820() {
        let game = new_scenario_game("1820", Difficulty::Normal, 0).unwrap();
        let gp_names: Vec<String> = game.great_powers().iter().map(|n| n.name.clone()).collect();
        assert_eq!(
            gp_names,
            vec![
                "Britain",
                "France",
                "Prussia",
                "Austria",
                "Russia",
                "Spain",
                "Ottoman Empire"
            ]
        );
    }

    #[test]
    fn scenario_1820_pre_researches_first_two_free_techs() {
        let game = new_scenario_game("1820", Difficulty::Normal, 0).unwrap();
        for nation in game.great_powers() {
            assert!(
                nation.has_researched(crate::events::TechId(1)),
                "{} should have researched High Pressure Steam Engine in 1820 scenario",
                nation.name
            );
            assert!(
                nation.has_researched(crate::events::TechId(2)),
                "{} should have researched Seed Drill in 1820 scenario",
                nation.name
            );
            // Should NOT have researched later techs
            assert_eq!(
                nation.researched_techs.len(),
                2,
                "{} should have exactly 2 pre-researched techs in 1820 scenario",
                nation.name
            );
        }
    }

    #[test]
    fn scenario_1815_does_not_pre_research_techs() {
        let game = new_scenario_game("1815", Difficulty::Normal, 0).unwrap();
        // 1815 is before 1848, so no pre-research should happen
        for nation in game.great_powers() {
            assert!(
                nation.researched_techs.is_empty(),
                "{} should not have pre-researched techs in 1815 scenario",
                nation.name
            );
        }
    }

    #[test]
    fn unknown_scenario_returns_error() {
        let result = new_scenario_game("9999", Difficulty::Normal, 0);
        assert!(result.is_err());
        match result {
            Err(e) => assert_eq!(e, "Unknown scenario: 9999"),
            Ok(_) => panic!("Expected error for unknown scenario"),
        }
    }

    #[test]
    fn scenario_game_has_7_great_powers() {
        let game = new_scenario_game("1815", Difficulty::Normal, 0).unwrap();
        assert_eq!(game.great_powers().len(), 7);
    }

    #[test]
    fn scenario_game_has_16_minor_nations() {
        let game = new_scenario_game("1815", Difficulty::Normal, 0).unwrap();
        assert_eq!(game.minor_nations().len(), 16);
    }

    #[test]
    fn scenario_human_nation_index_is_respected() {
        let game = new_scenario_game("1815", Difficulty::Normal, 2).unwrap();
        // The third Great Power should be the human player
        let gps = game.great_powers();
        assert_eq!(game.human_player_nation, gps[2].id);
    }
}
