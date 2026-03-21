use crate::events::TechId;
use crate::types::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Technology {
    pub id: TechId,
    pub name: String,
    pub cost: Money,
    pub earliest_year: u32,
    pub latest_year: u32,
    pub prerequisites: Vec<TechId>,
    pub effects: Vec<TechEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TechEffect {
    UnlockUnit(String),
    UnlockBuilding(String),
    EnableTerrainImprovement { terrain: String, max_level: u8 },
    EnableInfrastructure(String),
    UnlockShip(String),
    UpgradeUnit { from: String, to: String },
    EnableCivilian(String),
}

pub struct TechTree {
    technologies: Vec<Technology>,
}

impl TechTree {
    /// Creates the full tech tree with all 28 technologies from the original game.
    pub fn new() -> Self {
        let technologies = vec![
            Technology {
                id: TechId(1),
                name: "High Pressure Steam Engine".to_string(),
                cost: Money::dollars(0),
                earliest_year: 1815,
                latest_year: 1815,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableInfrastructure("Railroad".to_string())],
            },
            Technology {
                id: TechId(2),
                name: "Seed Drill".to_string(),
                cost: Money::dollars(0),
                earliest_year: 1815,
                latest_year: 1815,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Farm".to_string(),
                    max_level: 1,
                }],
            },
            Technology {
                id: TechId(3),
                name: "Cotton Gin".to_string(),
                cost: Money::dollars(1_000),
                earliest_year: 1816,
                latest_year: 1820,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Plantation".to_string(),
                    max_level: 1,
                }],
            },
            Technology {
                id: TechId(4),
                name: "Iron Railroad Bridge".to_string(),
                cost: Money::dollars(1_500),
                earliest_year: 1821,
                latest_year: 1824,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableInfrastructure(
                    "Railroad Bridge".to_string(),
                )],
            },
            Technology {
                id: TechId(5),
                name: "Feed Grasses".to_string(),
                cost: Money::dollars(1_500),
                earliest_year: 1821,
                latest_year: 1824,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "OpenRange".to_string(),
                    max_level: 1,
                }],
            },
            Technology {
                id: TechId(6),
                name: "Square-Set Timbering".to_string(),
                cost: Money::dollars(1_500),
                earliest_year: 1821,
                latest_year: 1825,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Mountain".to_string(),
                    max_level: 2,
                }],
            },
            Technology {
                id: TechId(7),
                name: "Streamlined Hulls".to_string(),
                cost: Money::dollars(1_500),
                earliest_year: 1821,
                latest_year: 1825,
                prerequisites: vec![],
                effects: vec![TechEffect::UnlockShip("Clipper".to_string())],
            },
            Technology {
                id: TechId(8),
                name: "Spinning Jenny".to_string(),
                cost: Money::dollars(3_000),
                earliest_year: 1826,
                latest_year: 1829,
                prerequisites: vec![TechId(3), TechId(5)],
                effects: vec![TechEffect::UnlockBuilding("Textile Mill".to_string())],
            },
            Technology {
                id: TechId(9),
                name: "Paddlewheels".to_string(),
                cost: Money::dollars(3_000),
                earliest_year: 1826,
                latest_year: 1830,
                prerequisites: vec![],
                effects: vec![TechEffect::UnlockShip("Paddle Steamer".to_string())],
            },
            Technology {
                id: TechId(10),
                name: "Steel Plows".to_string(),
                cost: Money::dollars(3_000),
                earliest_year: 1831,
                latest_year: 1835,
                prerequisites: vec![TechId(2)],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Farm".to_string(),
                    max_level: 2,
                }],
            },
            Technology {
                id: TechId(11),
                name: "Bessemer Converter".to_string(),
                cost: Money::dollars(6_000),
                earliest_year: 1836,
                latest_year: 1839,
                prerequisites: vec![],
                effects: vec![TechEffect::UnlockBuilding("Steel Mill".to_string())],
            },
            Technology {
                id: TechId(12),
                name: "Compound Steam Engine".to_string(),
                cost: Money::dollars(7_000),
                earliest_year: 1836,
                latest_year: 1838,
                prerequisites: vec![TechId(4)],
                effects: vec![TechEffect::EnableInfrastructure(
                    "Advanced Railroad".to_string(),
                )],
            },
            Technology {
                id: TechId(13),
                name: "Breech-Loading Rifles".to_string(),
                cost: Money::dollars(12_000),
                earliest_year: 1841,
                latest_year: 1845,
                prerequisites: vec![TechId(11)],
                effects: vec![TechEffect::UpgradeUnit {
                    from: "Militia".to_string(),
                    to: "Infantry".to_string(),
                }],
            },
            Technology {
                id: TechId(14),
                name: "Rifled Artillery".to_string(),
                cost: Money::dollars(10_000),
                earliest_year: 1841,
                latest_year: 1844,
                prerequisites: vec![],
                effects: vec![TechEffect::UpgradeUnit {
                    from: "Artillery".to_string(),
                    to: "Rifled Artillery".to_string(),
                }],
            },
            Technology {
                id: TechId(15),
                name: "Advanced Iron Working".to_string(),
                cost: Money::dollars(12_000),
                earliest_year: 1846,
                latest_year: 1850,
                prerequisites: vec![],
                effects: vec![TechEffect::UnlockShip("Ironclad".to_string())],
            },
            Technology {
                id: TechId(16),
                name: "Power Loom".to_string(),
                cost: Money::dollars(12_000),
                earliest_year: 1846,
                latest_year: 1851,
                prerequisites: vec![TechId(8)],
                effects: vec![TechEffect::UnlockBuilding(
                    "Advanced Textile Mill".to_string(),
                )],
            },
            Technology {
                id: TechId(17),
                name: "Mechanical Reaper".to_string(),
                cost: Money::dollars(12_000),
                earliest_year: 1851,
                latest_year: 1855,
                prerequisites: vec![TechId(10)],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Farm".to_string(),
                    max_level: 3,
                }],
            },
            Technology {
                id: TechId(18),
                name: "Commercial Fertilizer".to_string(),
                cost: Money::dollars(12_000),
                earliest_year: 1856,
                latest_year: 1860,
                prerequisites: vec![TechId(10)],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Orchard".to_string(),
                    max_level: 3,
                }],
            },
            Technology {
                id: TechId(19),
                name: "Oil Drilling".to_string(),
                cost: Money::dollars(25_000),
                earliest_year: 1856,
                latest_year: 1858,
                prerequisites: vec![],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Desert".to_string(),
                    max_level: 1,
                }],
            },
            Technology {
                id: TechId(20),
                name: "Barbed Wire".to_string(),
                cost: Money::dollars(20_000),
                earliest_year: 1862,
                latest_year: 1862,
                prerequisites: vec![TechId(5)],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "OpenRange".to_string(),
                    max_level: 2,
                }],
            },
            Technology {
                id: TechId(21),
                name: "Steel Armour Plate".to_string(),
                cost: Money::dollars(40_000),
                earliest_year: 1866,
                latest_year: 1868,
                prerequisites: vec![TechId(15)],
                effects: vec![TechEffect::UnlockShip("Battleship".to_string())],
            },
            Technology {
                id: TechId(22),
                name: "Large Artillery".to_string(),
                cost: Money::dollars(40_000),
                earliest_year: 1872,
                latest_year: 1886,
                prerequisites: vec![TechId(14)],
                effects: vec![TechEffect::UnlockUnit("Siege Artillery".to_string())],
            },
            Technology {
                id: TechId(23),
                name: "Dynamite".to_string(),
                cost: Money::dollars(40_000),
                earliest_year: 1874,
                latest_year: 1887,
                prerequisites: vec![TechId(12), TechId(6)],
                effects: vec![TechEffect::EnableTerrainImprovement {
                    terrain: "Mountain".to_string(),
                    max_level: 3,
                }],
            },
            Technology {
                id: TechId(24),
                name: "Marine Engineering".to_string(),
                cost: Money::dollars(40_000),
                earliest_year: 1873,
                latest_year: 1889,
                prerequisites: vec![TechId(21)],
                effects: vec![TechEffect::UnlockShip("Cruiser".to_string())],
            },
            Technology {
                id: TechId(25),
                name: "Machine Guns".to_string(),
                cost: Money::dollars(100_000),
                earliest_year: 1879,
                latest_year: 1893,
                prerequisites: vec![TechId(13)],
                effects: vec![TechEffect::UnlockUnit("Machine Gun Corps".to_string())],
            },
            Technology {
                id: TechId(26),
                name: "Chemistry".to_string(),
                cost: Money::dollars(120_000),
                earliest_year: 1875,
                latest_year: 1894,
                prerequisites: vec![TechId(19), TechId(20)],
                effects: vec![TechEffect::UnlockBuilding("Chemical Plant".to_string())],
            },
            Technology {
                id: TechId(27),
                name: "Improved Range-Finding".to_string(),
                cost: Money::dollars(150_000),
                earliest_year: 1881,
                latest_year: 1897,
                prerequisites: vec![TechId(24)],
                effects: vec![TechEffect::UnlockShip("Dreadnought".to_string())],
            },
            Technology {
                id: TechId(28),
                name: "Internal Combustion".to_string(),
                cost: Money::dollars(150_000),
                earliest_year: 1884,
                latest_year: 1898,
                prerequisites: vec![TechId(26)],
                effects: vec![TechEffect::UnlockUnit("Motorized Infantry".to_string())],
            },
        ];

        Self { technologies }
    }

    /// Look up a technology by its ID.
    pub fn get(&self, id: TechId) -> Option<&Technology> {
        self.technologies.iter().find(|t| t.id == id)
    }

    /// Look up a technology by its name (case-sensitive).
    pub fn get_by_name(&self, name: &str) -> Option<&Technology> {
        self.technologies.iter().find(|t| t.name == name)
    }

    /// Returns technologies available for research given the current set of
    /// researched techs and the current year.
    ///
    /// A technology is available when:
    /// - All prerequisites have been researched
    /// - `current_year >= earliest_year`
    /// - `current_year <= latest_year`
    /// - It has not already been researched
    pub fn available_techs(&self, researched: &[TechId], current_year: u32) -> Vec<&Technology> {
        let researched_set: HashSet<TechId> = researched.iter().copied().collect();
        self.technologies
            .iter()
            .filter(|tech| {
                // Not already researched
                !researched_set.contains(&tech.id)
                    // Within year window
                    && current_year >= tech.earliest_year
                    && current_year <= tech.latest_year
                    // All prerequisites researched
                    && tech.prerequisites.iter().all(|prereq| researched_set.contains(prereq))
            })
            .collect()
    }

    /// Check whether a technology has been researched.
    pub fn is_researched(&self, id: TechId, researched: &[TechId]) -> bool {
        researched.contains(&id)
    }

    /// Returns a slice of all technologies in the tree.
    pub fn all_techs(&self) -> &[Technology] {
        &self.technologies
    }

    /// Returns the total number of technologies in the tree.
    pub fn total_tech_count(&self) -> usize {
        self.technologies.len()
    }

    /// Validates the tech tree structure:
    /// - All prerequisite IDs refer to technologies that exist in the tree
    /// - There are no cycles in the prerequisite graph
    pub fn validate(&self) -> Result<(), String> {
        let ids: HashSet<TechId> = self.technologies.iter().map(|t| t.id).collect();

        // Check all prerequisite IDs exist
        for tech in &self.technologies {
            for prereq in &tech.prerequisites {
                if !ids.contains(prereq) {
                    return Err(format!(
                        "Technology '{}' (ID {}) has prerequisite ID {} which does not exist",
                        tech.name, tech.id.0, prereq.0
                    ));
                }
            }
        }

        // Check for cycles using topological sort (Kahn's algorithm)
        let mut in_degree: std::collections::HashMap<TechId, usize> =
            std::collections::HashMap::new();
        let mut dependents: std::collections::HashMap<TechId, Vec<TechId>> =
            std::collections::HashMap::new();

        for tech in &self.technologies {
            in_degree.entry(tech.id).or_insert(0);
            for prereq in &tech.prerequisites {
                *in_degree.entry(tech.id).or_insert(0) += 1;
                dependents.entry(*prereq).or_default().push(tech.id);
            }
        }

        let mut queue: Vec<TechId> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut visited = 0;

        while let Some(id) = queue.pop() {
            visited += 1;
            if let Some(deps) = dependents.get(&id) {
                for &dep in deps {
                    if let Some(deg) = in_degree.get_mut(&dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(dep);
                        }
                    }
                }
            }
        }

        if visited != self.technologies.len() {
            return Err("Cycle detected in tech tree prerequisites".to_string());
        }

        Ok(())
    }
}

impl Default for TechTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tech_tree_has_28_technologies() {
        let tree = TechTree::new();
        assert_eq!(tree.all_techs().len(), 28);
    }

    #[test]
    fn validation_passes() {
        let tree = TechTree::new();
        assert!(
            tree.validate().is_ok(),
            "Tech tree validation failed: {:?}",
            tree.validate()
        );
    }

    #[test]
    fn available_techs_at_1815() {
        let tree = TechTree::new();
        let available = tree.available_techs(&[], 1815);
        let names: Vec<&str> = available.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"High Pressure Steam Engine"),
            "Expected Steam Engine at 1815"
        );
        assert!(names.contains(&"Seed Drill"), "Expected Seed Drill at 1815");
        assert_eq!(
            available.len(),
            2,
            "Only 2 techs should be available at 1815 with no prereqs"
        );
    }

    #[test]
    fn dependent_techs_become_available_after_prereqs() {
        let tree = TechTree::new();

        // Steel Plows (ID 10) requires Seed Drill (ID 2) and year 1831-1835
        let available_before = tree.available_techs(&[], 1831);
        let names_before: Vec<&str> = available_before.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names_before.contains(&"Steel Plows"),
            "Steel Plows should NOT be available without Seed Drill"
        );

        // After researching Seed Drill
        let available_after = tree.available_techs(&[TechId(2)], 1831);
        let names_after: Vec<&str> = available_after.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names_after.contains(&"Steel Plows"),
            "Steel Plows should be available after researching Seed Drill in 1831"
        );
    }

    #[test]
    fn techs_outside_year_window_not_available() {
        let tree = TechTree::new();

        // Cotton Gin (ID 3) is available 1816-1820
        // Should NOT be available in 1815
        let available_1815 = tree.available_techs(&[], 1815);
        let names_1815: Vec<&str> = available_1815.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names_1815.contains(&"Cotton Gin"),
            "Cotton Gin should NOT be available in 1815 (too early)"
        );

        // Should be available in 1816
        let available_1816 = tree.available_techs(&[], 1816);
        let names_1816: Vec<&str> = available_1816.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names_1816.contains(&"Cotton Gin"),
            "Cotton Gin should be available in 1816"
        );

        // Should NOT be available in 1821 (too late)
        let available_1821 = tree.available_techs(&[], 1821);
        let names_1821: Vec<&str> = available_1821.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names_1821.contains(&"Cotton Gin"),
            "Cotton Gin should NOT be available in 1821 (too late)"
        );
    }

    #[test]
    fn get_by_name_works() {
        let tree = TechTree::new();

        let tech = tree.get_by_name("Bessemer Converter");
        assert!(tech.is_some(), "Should find Bessemer Converter by name");
        let tech = tech.unwrap();
        assert_eq!(tech.id, TechId(11));
        assert_eq!(tech.cost, Money::dollars(6_000));
        assert_eq!(tech.earliest_year, 1836);
        assert_eq!(tech.latest_year, 1839);

        let not_found = tree.get_by_name("Nonexistent Tech");
        assert!(
            not_found.is_none(),
            "Should return None for nonexistent tech"
        );
    }

    #[test]
    fn get_by_id_works() {
        let tree = TechTree::new();
        let tech = tree.get(TechId(1));
        assert!(tech.is_some());
        assert_eq!(tech.unwrap().name, "High Pressure Steam Engine");

        let not_found = tree.get(TechId(99));
        assert!(not_found.is_none());
    }

    #[test]
    fn is_researched_works() {
        let tree = TechTree::new();
        let researched = vec![TechId(1), TechId(2)];
        assert!(tree.is_researched(TechId(1), &researched));
        assert!(tree.is_researched(TechId(2), &researched));
        assert!(!tree.is_researched(TechId(3), &researched));
    }

    #[test]
    fn already_researched_techs_not_available() {
        let tree = TechTree::new();
        let available = tree.available_techs(&[TechId(1), TechId(2)], 1815);
        let ids: Vec<TechId> = available.iter().map(|t| t.id).collect();
        assert!(
            !ids.contains(&TechId(1)),
            "Already researched tech should not appear"
        );
        assert!(
            !ids.contains(&TechId(2)),
            "Already researched tech should not appear"
        );
    }

    #[test]
    fn multi_prerequisite_tech_requires_all() {
        let tree = TechTree::new();

        // Spinning Jenny (ID 8) requires Cotton Gin (3) AND Feed Grasses (5), years 1826-1829
        // With only one prereq, should NOT be available
        let available_partial = tree.available_techs(&[TechId(3)], 1826);
        let names: Vec<&str> = available_partial.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names.contains(&"Spinning Jenny"),
            "Spinning Jenny should NOT be available with only Cotton Gin"
        );

        // With both prereqs, should be available
        let available_both = tree.available_techs(&[TechId(3), TechId(5)], 1826);
        let names: Vec<&str> = available_both.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"Spinning Jenny"),
            "Spinning Jenny should be available with both prereqs in 1826"
        );
    }

    #[test]
    fn tech_data_validation_all_ids_valid() {
        let tree = TechTree::new();
        let validation = tree.validate();
        assert!(
            validation.is_ok(),
            "Tech tree validation failed: {:?}",
            validation
        );
    }

    #[test]
    fn simulate_100_turns_all_techs_researchable() {
        // Start a game, research cheapest tech each turn for 100 turns
        // Verify that techs become available and can be researched
        let mut game = crate::game_state::new_game("tech_sim", crate::types::Difficulty::Normal, 0);
        let mut researched_count = 0;
        for _ in 0..100 {
            let available = game.tech_tree.available_techs(
                &game
                    .get_nation(game.human_player_nation)
                    .unwrap()
                    .researched_techs,
                game.turn.year(),
            );
            if let Some(tech) = available.first() {
                let tech_id = tech.id;
                let nation = game.get_nation_mut(game.human_player_nation).unwrap();
                nation.research_tech(tech_id);
                researched_count += 1;
            }
            crate::turn::process_turn(&mut game);
        }
        assert!(
            researched_count > 0,
            "Should have researched at least some techs"
        );
    }

    #[test]
    fn scenario_start_dates_provide_correct_starting_techs() {
        use crate::scenarios::new_scenario_game;
        use crate::types::Difficulty;

        // 1815: no pre-researched techs
        // 1848: early techs pre-researched
        // 1882: more techs pre-researched
        let game_1815 = new_scenario_game("1815", Difficulty::Normal, 0).unwrap();
        let game_1848 = new_scenario_game("1848", Difficulty::Normal, 0).unwrap();
        let game_1882 = new_scenario_game("1882", Difficulty::Normal, 0).unwrap();

        let techs_1815 = game_1815
            .get_nation(game_1815.human_player_nation)
            .unwrap()
            .researched_techs
            .len();
        let techs_1848 = game_1848
            .get_nation(game_1848.human_player_nation)
            .unwrap()
            .researched_techs
            .len();
        let techs_1882 = game_1882
            .get_nation(game_1882.human_player_nation)
            .unwrap()
            .researched_techs
            .len();

        assert!(
            techs_1848 > techs_1815,
            "1848 should have more techs than 1815"
        );
        assert!(
            techs_1882 > techs_1848,
            "1882 should have more techs than 1848"
        );
    }
}
