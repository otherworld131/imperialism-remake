use crate::economy::buildings::BuildingType;
use crate::economy::civilians::CivilianType;
use crate::events::TechId;
use crate::military::units::ArmyUnitType;
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
    UnlockUnit(ArmyUnitType),
    UnlockBuilding(BuildingType),
    EnableTerrainImprovement {
        terrain: String,
        max_level: u8,
    },
    EnableInfrastructure(String),
    UnlockShip(String),
    UpgradeUnit {
        from: ArmyUnitType,
        to: ArmyUnitType,
    },
    EnableCivilian(CivilianType),
    /// Run a Lua script when this tech is researched.
    LuaScript(String),
}

#[derive(Clone)]
pub struct TechTree {
    technologies: Vec<Technology>,
}

impl TechTree {
    /// Creates a TechTree from a pre-built list of technologies.
    /// Used by the data loader to construct a tree from RON definitions.
    pub fn from_technologies(technologies: Vec<Technology>) -> Self {
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

    /// Highest improvement level the nation may take a tile to, given the set
    /// of techs it has researched. Walks `EnableTerrainImprovement` effects
    /// across all researched techs, taking the maximum `max_level` for the
    /// (terrain, resource) class.
    ///
    /// Returns:
    /// - The tech-gated cap for tiles in a known class (Farm, Orchard,
    ///   Plantation, Wool, Livestock, Forest, Mining, Oil).
    /// - The implicit Mining base of 1 when the nation has no relevant
    ///   tech yet — a Miner can always open a fresh mine to L1 per
    ///   manual p.27.
    /// - 0 if the tile has no resource or the resource has no class.
    pub fn effective_max_improvement_level(
        &self,
        terrain: TerrainType,
        resource: Option<ResourceType>,
        researched: &[TechId],
    ) -> u8 {
        let resource = match resource {
            Some(r) => r,
            None => return 0,
        };
        let class = match improvement_class(terrain, resource) {
            Some(c) => c,
            None => return 0,
        };
        // Manual p.27: "When a Miner finishes opening a new mine it produces
        // at Level I." Mining can reach L1 with no tech; all other classes
        // need an explicit `EnableTerrainImprovement` effect at L1+ to be
        // workable at all.
        let base = match class {
            "Mining" => 1,
            _ => 0,
        };
        let mut max = base;
        for tid in researched {
            if let Some(tech) = self.get(*tid) {
                for effect in &tech.effects {
                    if let TechEffect::EnableTerrainImprovement {
                        terrain: eff_class,
                        max_level,
                    } = effect
                        && eff_class == class
                        && *max_level > max
                    {
                        max = *max_level;
                    }
                }
            }
        }
        max.min(resource.max_improvement_level())
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
    /// - No duplicate tech IDs
    /// - All prerequisite IDs refer to technologies that exist in the tree
    /// - No year inversions (earliest_year <= latest_year)
    /// - No unreachable techs (all techs reachable from root techs via prerequisites)
    /// - There are no cycles in the prerequisite graph
    pub fn validate(&self) -> Result<(), String> {
        // Check for duplicate tech IDs
        let mut seen_ids: HashSet<TechId> = HashSet::new();
        for tech in &self.technologies {
            if !seen_ids.insert(tech.id) {
                return Err(format!(
                    "Duplicate technology ID {} (name: '{}')",
                    tech.id.0, tech.name
                ));
            }
        }

        let ids: &HashSet<TechId> = &seen_ids;

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

        // Check for year inversions
        for tech in &self.technologies {
            if tech.earliest_year > tech.latest_year {
                return Err(format!(
                    "Technology '{}' (ID {}) has earliest_year {} > latest_year {}",
                    tech.name, tech.id.0, tech.earliest_year, tech.latest_year
                ));
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

        // Check reachability via transitive timeline analysis.
        // Compute the effective earliest-reachable-year for each tech by propagating
        // through the prerequisite graph in topological order. A tech's effective
        // earliest year is max(its own earliest_year, max(prereq effective years)).
        // If effective_earliest > latest_year, the tech is unreachable.
        let tech_map: std::collections::HashMap<TechId, &Technology> =
            self.technologies.iter().map(|t| (t.id, t)).collect();

        let mut effective_earliest: std::collections::HashMap<TechId, u32> =
            std::collections::HashMap::new();

        // Re-run topological sort to get processing order
        let mut in_deg2: std::collections::HashMap<TechId, usize> =
            std::collections::HashMap::new();
        let mut deps2: std::collections::HashMap<TechId, Vec<TechId>> =
            std::collections::HashMap::new();

        for tech in &self.technologies {
            in_deg2.entry(tech.id).or_insert(0);
            for prereq in &tech.prerequisites {
                *in_deg2.entry(tech.id).or_insert(0) += 1;
                deps2.entry(*prereq).or_default().push(tech.id);
            }
        }

        let mut topo_queue: Vec<TechId> = in_deg2
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(&id, _)| id)
            .collect();

        // Root techs: effective earliest = their own earliest_year
        for &root_id in &topo_queue {
            if let Some(tech) = tech_map.get(&root_id) {
                effective_earliest.insert(root_id, tech.earliest_year);
            }
        }

        while let Some(id) = topo_queue.pop() {
            let eff = effective_earliest[&id];

            if let Some(tech) = tech_map.get(&id)
                && eff > tech.latest_year
            {
                return Err(format!(
                    "Technology '{}' (ID {}) is unreachable: effective earliest year {} \
                     (from transitive prerequisites) exceeds latest_year {}",
                    tech.name, tech.id.0, eff, tech.latest_year
                ));
            }

            if let Some(dep_ids) = deps2.get(&id) {
                for &dep in dep_ids {
                    // Propagate: dependent's effective earliest is at least as late
                    // as the max of all its prerequisites' effective earliest years
                    let dep_tech = tech_map[&dep];
                    let new_eff = eff.max(dep_tech.earliest_year);
                    let current = effective_earliest
                        .entry(dep)
                        .or_insert(dep_tech.earliest_year);
                    if new_eff > *current {
                        *current = new_eff;
                    }

                    if let Some(deg) = in_deg2.get_mut(&dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            topo_queue.push(dep);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for TechTree {
    /// An empty tree. Production builds populate the tree from
    /// `scripts/config/tech_tree.lua` via `lua_bridge::load_tech_tree`,
    /// which is the single source of truth.
    fn default() -> Self {
        Self::from_technologies(Vec::new())
    }
}

/// Map a (terrain, resource) pair to the tech-tree improvement class string
/// used by `EnableTerrainImprovement` effects.
///
/// Reference: original Imperialism manual p.89 (Benefits of Technology Table)
/// and p.27–28 (per-civilian sections).
///
/// Per the manual the Wool ladder (Feed Grasses → Spinning Jenny → Power
/// Loom) and the Livestock ladder (Feed Grasses → Barbed Wire → Chemistry)
/// diverge at L2, so they are separate classes.
fn improvement_class(terrain: TerrainType, resource: ResourceType) -> Option<&'static str> {
    use ResourceType::*;
    use TerrainType::*;
    match (terrain, resource) {
        (Grassland, Grain) => Some("Farm"),
        (Grassland, Fruit) => Some("Orchard"),
        (Grassland, Cotton) => Some("Plantation"),
        (Grassland, Livestock | Horses) => Some("Livestock"),
        (Hills, Wool) => Some("Wool"),
        // Hills mining and Mountain mining share Square-Set Timbering / Dynamite.
        (Hills | Mountain, Coal | Iron | Gold | Gems) => Some("Mining"),
        (Forest, Timber) => Some("Forest"),
        // All oil terrains share the Oil Drilling / Chemistry / Internal
        // Combustion ladder.
        (Desert | Swamp | Tundra, Oil) => Some("Oil"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct TechDefsFile {
        technologies: Vec<TechDef>,
    }

    #[derive(Debug, Deserialize)]
    struct TechDef {
        id: u32,
        name: String,
        cost: i64,
        earliest_year: u32,
        latest_year: u32,
        prerequisites: Vec<u32>,
        effects: Vec<TechEffectDef>,
    }

    #[derive(Debug, Deserialize)]
    enum TechEffectDef {
        UnlockUnit(String),
        UnlockBuilding(String),
        EnableTerrainImprovement { terrain: String, max_level: u8 },
        EnableInfrastructure(String),
        UnlockShip(String),
        UpgradeUnit { from: String, to: String },
        EnableCivilian(String),
        LuaScript(String),
    }

    fn convert_tech_effect(def: TechEffectDef) -> TechEffect {
        match def {
            TechEffectDef::UnlockUnit(name) => {
                TechEffect::UnlockUnit(name.parse().expect("valid army unit in test data"))
            }
            TechEffectDef::UnlockBuilding(name) => {
                TechEffect::UnlockBuilding(name.parse().expect("valid building type in test data"))
            }
            TechEffectDef::EnableTerrainImprovement { terrain, max_level } => {
                TechEffect::EnableTerrainImprovement { terrain, max_level }
            }
            TechEffectDef::EnableInfrastructure(name) => TechEffect::EnableInfrastructure(name),
            TechEffectDef::UnlockShip(name) => TechEffect::UnlockShip(name),
            TechEffectDef::UpgradeUnit { from, to } => TechEffect::UpgradeUnit {
                from: from.parse().expect("valid from-unit in test data"),
                to: to.parse().expect("valid to-unit in test data"),
            },
            TechEffectDef::EnableCivilian(name) => {
                TechEffect::EnableCivilian(name.parse().expect("valid civilian type in test data"))
            }
            TechEffectDef::LuaScript(script) => TechEffect::LuaScript(script),
        }
    }

    fn load_ron_tree() -> TechTree {
        let ron_content = include_str!("../../../../data/definitions/technologies.ron");
        let defs: TechDefsFile =
            ron::from_str(ron_content).expect("technologies.ron must be valid");
        let tree = TechTree::from_technologies(
            defs.technologies
                .into_iter()
                .map(|def| Technology {
                    id: TechId(def.id),
                    name: def.name,
                    cost: Money::dollars(def.cost),
                    earliest_year: def.earliest_year,
                    latest_year: def.latest_year,
                    prerequisites: def.prerequisites.into_iter().map(TechId).collect(),
                    effects: def.effects.into_iter().map(convert_tech_effect).collect(),
                })
                .collect(),
        );
        tree.validate()
            .expect("embedded tech tree test data should validate");
        tree
    }

    #[test]
    fn tech_tree_has_28_technologies() {
        let tree = load_ron_tree();
        assert_eq!(tree.all_techs().len(), 28);
    }

    // ── Tech-gated improvement levels ─────────────────────────────

    #[test]
    fn farm_max_level_starts_at_zero_then_unlocks_with_techs() {
        let tree = load_ron_tree();
        // No tech researched → cannot improve a Farm tile.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Grain),
                &[]
            ),
            0
        );
        // Seed Drill (id 2) → L1.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Grain),
                &[TechId(2)]
            ),
            1
        );
        // Steel Plows (id 10) → L2.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Grain),
                &[TechId(2), TechId(10)]
            ),
            2
        );
        // Mechanical Reaper (id 17) → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Grain),
                &[TechId(2), TechId(10), TechId(17)]
            ),
            3
        );
    }

    #[test]
    fn mountain_mining_baseline_is_l1_without_tech() {
        let tree = load_ron_tree();
        // Per manual p.27: a Miner can always open a fresh mine to L1.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Mountain,
                Some(ResourceType::Coal),
                &[]
            ),
            1
        );
        // Square-Set Timbering (id 6) → L2.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Mountain,
                Some(ResourceType::Coal),
                &[TechId(6)]
            ),
            2
        );
        // Dynamite (id 23) → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Mountain,
                Some(ResourceType::Iron),
                &[TechId(6), TechId(23)]
            ),
            3
        );
    }

    #[test]
    fn hills_mining_uses_same_class_as_mountain() {
        let tree = load_ron_tree();
        // Coal on Hills should follow the same Mountain ladder.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Hills,
                Some(ResourceType::Coal),
                &[TechId(6)]
            ),
            2,
            "Hills mining must share Mountain's tech gates"
        );
    }

    #[test]
    fn livestock_ladder_feed_grasses_barbed_wire_chemistry() {
        let tree = load_ron_tree();
        // No tech → 0.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Livestock),
                &[]
            ),
            0
        );
        // Feed Grasses (id 5) → L1.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Livestock),
                &[TechId(5)]
            ),
            1
        );
        // Barbed Wire (id 20) → L2.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Livestock),
                &[TechId(5), TechId(20)]
            ),
            2
        );
        // Chemistry (id 26) → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Livestock),
                &[TechId(5), TechId(20), TechId(26)]
            ),
            3
        );
    }

    #[test]
    fn wool_ladder_feed_grasses_spinning_jenny_power_loom() {
        let tree = load_ron_tree();
        // Wool diverges from Livestock at L2: Spinning Jenny (id 8) → L2,
        // Power Loom (id 16) → L3 — completely separate from the Livestock
        // ladder (Barbed Wire / Chemistry).
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Hills,
                Some(ResourceType::Wool),
                &[TechId(5), TechId(8)]
            ),
            2
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Hills,
                Some(ResourceType::Wool),
                &[TechId(5), TechId(8), TechId(16)]
            ),
            3
        );
        // Barbed Wire alone does not raise Wool past L1 (it's a Livestock tech).
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Hills,
                Some(ResourceType::Wool),
                &[TechId(5), TechId(20)]
            ),
            1
        );
    }

    #[test]
    fn cotton_plantation_ladder() {
        let tree = load_ron_tree();
        // Cotton Gin (id 3) → L1, Spinning Jenny (id 8) → L2, Power Loom (16) → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Cotton),
                &[TechId(3)]
            ),
            1
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Cotton),
                &[TechId(3), TechId(5), TechId(8)]
            ),
            2
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Cotton),
                &[TechId(3), TechId(5), TechId(8), TechId(16)]
            ),
            3
        );
    }

    #[test]
    fn orchard_fruit_ladder() {
        let tree = load_ron_tree();
        // Seed Drill (2) → L1, Steel and Iron Plows (10) → L2, Commercial Fertilizer (18) → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Fruit),
                &[TechId(2)]
            ),
            1
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Fruit),
                &[TechId(2), TechId(10)]
            ),
            2
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Fruit),
                &[TechId(2), TechId(10), TechId(18)]
            ),
            3
        );
        // Mechanical Reaper (Farm L3) does NOT raise Orchard.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Grassland,
                Some(ResourceType::Fruit),
                &[TechId(2), TechId(10), TechId(17)]
            ),
            2
        );
    }

    #[test]
    fn oil_ladder_drilling_chemistry_internal_combustion() {
        let tree = load_ron_tree();
        // No tech → 0 (cannot drill anywhere).
        assert_eq!(
            tree.effective_max_improvement_level(TerrainType::Desert, Some(ResourceType::Oil), &[]),
            0
        );
        // Oil Drilling (19) → L1.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Desert,
                Some(ResourceType::Oil),
                &[TechId(19)]
            ),
            1
        );
        // Chemistry (26) → L2 — also covers Swamp/Tundra.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Swamp,
                Some(ResourceType::Oil),
                &[TechId(19), TechId(20), TechId(26)]
            ),
            2
        );
        // Internal Combustion (28) → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Tundra,
                Some(ResourceType::Oil),
                &[TechId(19), TechId(20), TechId(26), TechId(28)]
            ),
            3
        );
    }

    #[test]
    fn forest_ladder_iron_rr_compound_steam_dynamite() {
        let tree = load_ron_tree();
        // Per manual p.27 + p.89: Forester not buildable until Iron Railroad
        // Bridge; that tech also unlocks Timber L1. Compound Steam Engine →
        // L2, Dynamite → L3.
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Forest,
                Some(ResourceType::Timber),
                &[]
            ),
            0,
            "no tech → no Timber improvement"
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Forest,
                Some(ResourceType::Timber),
                &[TechId(4)]
            ),
            1
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Forest,
                Some(ResourceType::Timber),
                &[TechId(1), TechId(4), TechId(12)]
            ),
            2
        );
        assert_eq!(
            tree.effective_max_improvement_level(
                TerrainType::Forest,
                Some(ResourceType::Timber),
                &[TechId(1), TechId(4), TechId(6), TechId(12), TechId(23)]
            ),
            3
        );
    }

    #[test]
    fn validation_passes() {
        let tree = load_ron_tree();
        assert!(
            tree.validate().is_ok(),
            "Tech tree validation failed: {:?}",
            tree.validate()
        );
    }

    #[test]
    fn available_techs_at_1815() {
        let tree = load_ron_tree();
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
        let tree = load_ron_tree();

        // Steel and Iron Plows (ID 10) requires Seed Drill (ID 2) and year 1831-1835
        let available_before = tree.available_techs(&[], 1831);
        let names_before: Vec<&str> = available_before.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !names_before.contains(&"Steel and Iron Plows"),
            "Steel and Iron Plows should NOT be available without Seed Drill"
        );

        // After researching Seed Drill
        let available_after = tree.available_techs(&[TechId(2)], 1831);
        let names_after: Vec<&str> = available_after.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names_after.contains(&"Steel and Iron Plows"),
            "Steel and Iron Plows should be available after researching Seed Drill in 1831"
        );
    }

    #[test]
    fn techs_outside_year_window_not_available() {
        let tree = load_ron_tree();

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
        let tree = load_ron_tree();

        let tech = tree.get_by_name("Bessemer Converter");
        assert!(tech.is_some(), "Should find Bessemer Converter by name");
        let tech = tech.unwrap();
        assert_eq!(tech.id, TechId(11));
        assert_eq!(tech.cost, Money::dollars(6_000));
        assert_eq!(tech.earliest_year, 1836);
        assert_eq!(tech.latest_year, 1840);

        let not_found = tree.get_by_name("Nonexistent Tech");
        assert!(
            not_found.is_none(),
            "Should return None for nonexistent tech"
        );
    }

    #[test]
    fn get_by_id_works() {
        let tree = load_ron_tree();
        let tech = tree.get(TechId(1));
        assert!(tech.is_some());
        assert_eq!(tech.unwrap().name, "High Pressure Steam Engine");

        let not_found = tree.get(TechId(99));
        assert!(not_found.is_none());
    }

    #[test]
    fn is_researched_works() {
        let tree = load_ron_tree();
        let researched = vec![TechId(1), TechId(2)];
        assert!(tree.is_researched(TechId(1), &researched));
        assert!(tree.is_researched(TechId(2), &researched));
        assert!(!tree.is_researched(TechId(3), &researched));
    }

    #[test]
    fn already_researched_techs_not_available() {
        let tree = load_ron_tree();
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
        let tree = load_ron_tree();

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
        let tree = load_ron_tree();
        let validation = tree.validate();
        assert!(
            validation.is_ok(),
            "Tech tree validation failed: {:?}",
            validation
        );
    }

    #[test]
    fn simulate_30_turns_all_techs_researchable() {
        // Start a game, research cheapest tech each turn for 30 turns
        // Verify that techs become available and can be researched
        let mut game = crate::game_state::new_game_with_data(
            "tech_sim",
            crate::types::Difficulty::Normal,
            0,
            crate::data::test_game_data(),
        );
        let mut researched_count = 0;
        for _ in 0..30 {
            let available = game.game_data.tech_tree.available_techs(
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
            researched_count >= 3,
            "Should have researched at least 3 techs in 30 turns, got {}",
            researched_count
        );
    }

    #[test]
    fn scenario_start_dates_provide_correct_starting_techs() {
        use crate::scenarios::new_scenario_game_with_data;
        use crate::types::Difficulty;

        // 1815: no pre-researched techs
        // 1848: early techs pre-researched
        // 1882: more techs pre-researched
        let game_1815 = new_scenario_game_with_data(
            "1815",
            Difficulty::Normal,
            0,
            crate::data::test_game_data(),
        )
        .unwrap();
        let game_1848 = new_scenario_game_with_data(
            "1848",
            Difficulty::Normal,
            0,
            crate::data::test_game_data(),
        )
        .unwrap();
        let game_1882 = new_scenario_game_with_data(
            "1882",
            Difficulty::Normal,
            0,
            crate::data::test_game_data(),
        )
        .unwrap();

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

    #[test]
    fn validate_rejects_duplicate_ids() {
        let techs = vec![
            Technology {
                id: TechId(1),
                name: "Tech A".to_string(),
                cost: Money::dollars(100),
                earliest_year: 1800,
                latest_year: 1850,
                prerequisites: vec![],
                effects: vec![],
            },
            Technology {
                id: TechId(1), // duplicate
                name: "Tech B".to_string(),
                cost: Money::dollars(200),
                earliest_year: 1800,
                latest_year: 1850,
                prerequisites: vec![],
                effects: vec![],
            },
        ];
        let tree = TechTree::from_technologies(techs);
        let result = tree.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate technology ID"));
    }

    #[test]
    fn validate_rejects_year_inversion() {
        let techs = vec![Technology {
            id: TechId(1),
            name: "Inverted".to_string(),
            cost: Money::dollars(100),
            earliest_year: 1850,
            latest_year: 1800, // inverted
            prerequisites: vec![],
            effects: vec![],
        }];
        let tree = TechTree::from_technologies(techs);
        let result = tree.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("earliest_year"));
    }

    #[test]
    fn validate_rejects_unreachable_tech() {
        let techs = vec![
            Technology {
                id: TechId(1),
                name: "Root".to_string(),
                cost: Money::dollars(0),
                earliest_year: 1800,
                latest_year: 1810,
                prerequisites: vec![],
                effects: vec![],
            },
            Technology {
                id: TechId(2),
                name: "Unreachable".to_string(),
                cost: Money::dollars(100),
                earliest_year: 1800,
                latest_year: 1805, // latest_year < Root's earliest_year is fine, but
                // prereq Root earliest=1800, this latest=1805 — Root CAN be done by 1805
                prerequisites: vec![TechId(1)],
                effects: vec![],
            },
        ];
        // This should pass since Root (1800-1810) overlaps with Unreachable (1800-1805)
        let tree = TechTree::from_technologies(techs);
        assert!(tree.validate().is_ok());

        // Now make it truly unreachable: prereq only available AFTER dependent expires
        let techs2 = vec![
            Technology {
                id: TechId(1),
                name: "Late Root".to_string(),
                cost: Money::dollars(0),
                earliest_year: 1850,
                latest_year: 1900,
                prerequisites: vec![],
                effects: vec![],
            },
            Technology {
                id: TechId(2),
                name: "Unreachable".to_string(),
                cost: Money::dollars(100),
                earliest_year: 1800,
                latest_year: 1840, // expires before prereq is available
                prerequisites: vec![TechId(1)],
                effects: vec![],
            },
        ];
        let tree2 = TechTree::from_technologies(techs2);
        let result = tree2.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unreachable"));
    }

    #[test]
    fn validate_rejects_transitive_unreachable() {
        // A -> B -> C where A is late, B passes pairwise with A,
        // but C's latest_year is before A's earliest_year (transitive failure)
        let techs = vec![
            Technology {
                id: TechId(1),
                name: "Late Root".to_string(),
                cost: Money::dollars(0),
                earliest_year: 1850,
                latest_year: 1900,
                prerequisites: vec![],
                effects: vec![],
            },
            Technology {
                id: TechId(2),
                name: "Middle".to_string(),
                cost: Money::dollars(100),
                earliest_year: 1840,
                latest_year: 1900, // passes pairwise: Root earliest(1850) <= Middle latest(1900)
                prerequisites: vec![TechId(1)],
                effects: vec![],
            },
            Technology {
                id: TechId(3),
                name: "End".to_string(),
                cost: Money::dollars(200),
                earliest_year: 1840,
                latest_year: 1845, // passes pairwise with Middle (1840 <= 1845)
                // but transitively: effective earliest from Root = 1850 > 1845
                prerequisites: vec![TechId(2)],
                effects: vec![],
            },
        ];
        let tree = TechTree::from_technologies(techs);
        let result = tree.validate();
        assert!(result.is_err(), "Should catch transitive unreachable tech");
        assert!(result.unwrap_err().contains("unreachable"));
    }

    #[test]
    fn validate_ron_tree_passes() {
        let tree = load_ron_tree();
        assert!(tree.validate().is_ok(), "RON tech tree should be valid");
    }
}
