use crate::types::*;

// ── UnitId ─────────────────────────────────────────────────────

/// Unique identifier for a unit (civilian or military).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitId(pub u32);

impl std::fmt::Display for UnitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UnitId({})", self.0)
    }
}

// ── Infrastructure ─────────────────────────────────────────────

/// Infrastructure built on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Infrastructure {
    pub has_railroad: bool,
    pub has_depot: bool,
    pub has_port: bool,
    pub has_fort: bool,
    /// Fort level (0 = no fort, 1-3 = fortification levels). Only meaningful when `has_fort` is true.
    pub fort_level: u8,
}

impl Infrastructure {
    pub const NONE: Self = Self {
        has_railroad: false,
        has_depot: false,
        has_port: false,
        has_fort: false,
        fort_level: 0,
    };
}

impl Default for Infrastructure {
    fn default() -> Self {
        Self::NONE
    }
}

// ── Tile ───────────────────────────────────────────────────────

/// A single hex tile on the game map.
///
/// Terrain describes the landscape (Grassland, Hills, Forest, etc.).
/// Resources are an optional overlay — most tiles have no resource.
/// Hidden resources (Coal, Iron, Gold, Gems, Oil) require prospecting to reveal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    /// The landscape terrain type (immutable after creation).
    terrain: TerrainType,

    /// Resource on this tile (if any). For surface resources (Grain, Timber, etc.)
    /// this is set at map generation. For hidden resources (Coal, Iron, etc.)
    /// this starts as `None` until a prospector reveals it.
    resource_deposit: Option<ResourceType>,

    /// Whether this tile has been prospected (only meaningful for deposit-capable terrain).
    prospected: bool,

    /// Improvement level: 0 (unimproved) through 3 (fully developed).
    improvement_level: u8,

    /// Infrastructure built on this tile.
    pub infrastructure: Infrastructure,

    /// The civilian unit currently working this tile, if any.
    pub assigned_civilian: Option<UnitId>,

    /// The province this tile belongs to.
    pub province_id: Option<ProvinceId>,

    /// Whether this tile is the capital of its province.
    pub is_capital: bool,

    /// Whether this tile is (or was originally) a country/nation capital.
    /// Distinct from `is_capital` — that flag marks every province's centroid.
    /// Set once at game setup and never cleared, so captured foreign capitals
    /// continue to function as implicit depots and rail-network seeds even
    /// after they change hands.
    pub is_country_capital: bool,
}

impl Tile {
    /// Create a new tile with the given terrain type. Starts with no resource.
    pub fn new(terrain: TerrainType) -> Self {
        Self {
            terrain,
            resource_deposit: None,
            prospected: false,
            improvement_level: 0,
            infrastructure: Infrastructure::NONE,
            assigned_civilian: None,
            province_id: None,
            is_capital: false,
            is_country_capital: false,
        }
    }

    /// Create a new tile with terrain and a visible resource.
    pub fn with_resource(terrain: TerrainType, resource: ResourceType) -> Self {
        Self {
            resource_deposit: Some(resource),
            ..Self::new(terrain)
        }
    }

    /// Create a new tile assigned to a specific province.
    pub fn with_province(terrain: TerrainType, province_id: ProvinceId) -> Self {
        Self {
            province_id: Some(province_id),
            ..Self::new(terrain)
        }
    }

    // ── Getters ────────────────────────────────────────────────

    /// The landscape terrain type of this tile.
    pub fn terrain(&self) -> TerrainType {
        self.terrain
    }

    /// The resource on this tile (if any).
    pub fn resource_deposit(&self) -> Option<ResourceType> {
        self.resource_deposit
    }

    /// Current improvement level (0-3).
    pub fn improvement_level(&self) -> u8 {
        self.improvement_level
    }

    // ── Resource management ───────────────────────────────────

    /// Place a visible resource on this tile (used during map generation).
    pub fn set_resource(&mut self, resource: ResourceType) {
        self.resource_deposit = Some(resource);
    }

    /// Reveal a hidden deposit via prospecting.
    pub fn reveal_deposit(&mut self, resource: ResourceType) {
        self.resource_deposit = Some(resource);
        self.prospected = true;
    }

    /// Prospecting found nothing.
    pub fn reveal_no_deposit(&mut self) {
        self.resource_deposit = None;
        self.prospected = true;
    }

    /// Whether this tile has been prospected.
    pub fn is_prospected(&self) -> bool {
        if self.terrain.can_have_deposits() {
            self.prospected
        } else {
            true
        }
    }

    /// Whether this tile has a resource visible to the player.
    /// Surface resources (Grain, Timber, etc.) are always visible.
    /// Hidden resources (Coal, Iron, etc.) are visible only after prospecting.
    pub fn has_visible_resource(&self) -> bool {
        match self.resource_deposit {
            None => false,
            Some(r) => {
                if r.requires_prospecting() {
                    self.prospected
                } else {
                    true
                }
            }
        }
    }

    // ── Improvement ────────────────────────────────────────────

    /// Attempt to improve this tile by one level.
    /// Returns `true` if improvement succeeded.
    pub fn improve(&mut self) -> bool {
        let max = self
            .resource_deposit
            .map(|r| r.max_improvement_level())
            .unwrap_or(0);
        if self.improvement_level < max {
            self.improvement_level += 1;
            true
        } else {
            false
        }
    }

    /// Set the improvement level directly. Clamps to the resource's maximum.
    pub fn set_improvement_level(&mut self, level: u8) {
        let max = self
            .resource_deposit
            .map(|r| r.max_improvement_level())
            .unwrap_or(0);
        self.improvement_level = level.min(max);
    }

    // ── Yield calculation ──────────────────────────────────────

    /// Calculate the resource yield of this tile based on resource and improvement level.
    ///
    /// Returns `None` if the tile has no resource or the resource isn't yet productive.
    ///
    /// ## Yield Rules
    ///
    /// Matches the original Imperialism (1997) Resource Development Table (manual p.28):
    ///
    /// - **Surface resources** (Grain, Fruit, Cotton, Wool, Timber, Livestock, Horses):
    ///   1 / 2 / 3 / 4 by level (always visible, baseline at level 0).
    /// - **Coal / Iron**: 0 / 2 / 4 / 6 by level. Hidden until prospected; a Miner builds
    ///   the mine to reach level 1.
    /// - **Gold / Gems**: 0 / 1 / 2 / 3 by level. Hidden until prospected; mined to L1.
    /// - **Oil**: 0 / 2 / 4 / 6 by level. Hidden until prospected (gated by Oil Drilling
    ///   tech); a Driller builds the derrick to reach level 1.
    pub fn calculate_yield(&self) -> Option<ResourceAmount> {
        let resource = self.resource_deposit?;
        let level = self.improvement_level as u32;

        match resource {
            // Coal, Iron, Oil: 0 / 2 / 4 / 6
            ResourceType::Coal | ResourceType::Iron | ResourceType::Oil => {
                if level == 0 {
                    None
                } else {
                    Some(ResourceAmount::new(resource, 2 * level))
                }
            }

            // Gold and Gems: 0 / 1 / 2 / 3
            ResourceType::Gold | ResourceType::Gems => {
                if level == 0 {
                    None
                } else {
                    Some(ResourceAmount::new(resource, level))
                }
            }

            // Surface resources: 1 / 2 / 3 / 4
            _ => Some(ResourceAmount::new(resource, 1 + level)),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_tile_is_unimproved() {
        let tile = Tile::new(TerrainType::Grassland);
        assert_eq!(tile.terrain(), TerrainType::Grassland);
        assert_eq!(tile.improvement_level(), 0);
        assert_eq!(tile.resource_deposit(), None);
        assert!(!tile.is_capital);
        assert_eq!(tile.province_id, None);
        assert_eq!(tile.assigned_civilian, None);
    }

    #[test]
    fn tile_with_province() {
        let tile = Tile::with_province(TerrainType::Mountain, ProvinceId(5));
        assert_eq!(tile.province_id, Some(ProvinceId(5)));
        assert_eq!(tile.terrain(), TerrainType::Mountain);
    }

    #[test]
    fn new_tile_has_no_infrastructure() {
        let tile = Tile::new(TerrainType::Grassland);
        assert!(!tile.infrastructure.has_railroad);
        assert!(!tile.infrastructure.has_depot);
        assert!(!tile.infrastructure.has_port);
        assert!(!tile.infrastructure.has_fort);
        assert_eq!(tile.infrastructure.fort_level, 0);
    }

    // ── UnitId ─────────────────────────────────────────────────

    #[test]
    fn unit_id_equality() {
        assert_eq!(UnitId(1), UnitId(1));
        assert_ne!(UnitId(1), UnitId(2));
    }

    #[test]
    fn unit_id_display() {
        assert_eq!(format!("{}", UnitId(42)), "UnitId(42)");
    }

    #[test]
    fn unit_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(UnitId(1));
        set.insert(UnitId(1));
        assert_eq!(set.len(), 1);
    }

    // ── Infrastructure ─────────────────────────────────────────

    #[test]
    fn infrastructure_default_is_none() {
        let infra = Infrastructure::default();
        assert_eq!(infra, Infrastructure::NONE);
    }

    #[test]
    fn infrastructure_can_be_set() {
        let mut tile = Tile::new(TerrainType::Grassland);
        tile.infrastructure.has_railroad = true;
        tile.infrastructure.has_depot = true;
        assert!(tile.infrastructure.has_railroad);
        assert!(tile.infrastructure.has_depot);
    }

    #[test]
    fn infrastructure_fort_with_level() {
        let mut tile = Tile::new(TerrainType::Hills);
        tile.infrastructure.has_fort = true;
        tile.infrastructure.fort_level = 2;
        assert!(tile.infrastructure.has_fort);
        assert_eq!(tile.infrastructure.fort_level, 2);
    }

    // ── Improvement ────────────────────────────────────────────

    #[test]
    fn improve_tile_with_resource_increases_level() {
        let mut tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Grain);
        assert!(tile.improve());
        assert_eq!(tile.improvement_level(), 1);
        assert!(tile.improve());
        assert_eq!(tile.improvement_level(), 2);
        assert!(tile.improve());
        assert_eq!(tile.improvement_level(), 3);
        // Max level reached
        assert!(!tile.improve());
        assert_eq!(tile.improvement_level(), 3);
    }

    #[test]
    fn improve_tile_without_resource_fails() {
        let mut tile = Tile::new(TerrainType::Forest);
        assert!(!tile.improve());
        assert_eq!(tile.improvement_level(), 0);
    }

    #[test]
    fn improve_grassland_without_resource_fails() {
        let mut tile = Tile::new(TerrainType::Grassland);
        assert!(!tile.improve());
    }

    #[test]
    fn improve_sea_fails() {
        let mut tile = Tile::new(TerrainType::Sea);
        assert!(!tile.improve());
    }

    #[test]
    fn set_improvement_level_clamps_to_max() {
        let mut tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Grain);
        tile.set_improvement_level(10);
        assert_eq!(tile.improvement_level(), 3); // Grain max is 3
    }

    #[test]
    fn set_improvement_level_on_tile_without_resource() {
        let mut tile = Tile::new(TerrainType::Forest);
        tile.set_improvement_level(5);
        assert_eq!(tile.improvement_level(), 0); // no resource means max is 0
    }

    // ── Prospecting ────────────────────────────────────────────

    #[test]
    fn reveal_deposit_on_mountain() {
        let mut tile = Tile::new(TerrainType::Mountain);
        assert!(!tile.is_prospected());
        tile.reveal_deposit(ResourceType::Iron);
        assert!(tile.is_prospected());
        assert_eq!(tile.resource_deposit(), Some(ResourceType::Iron));
    }

    #[test]
    fn reveal_deposit_on_hills() {
        let mut tile = Tile::new(TerrainType::Hills);
        tile.reveal_deposit(ResourceType::Coal);
        assert_eq!(tile.resource_deposit(), Some(ResourceType::Coal));
    }

    #[test]
    fn reveal_no_deposit() {
        let mut tile = Tile::new(TerrainType::Desert);
        tile.reveal_no_deposit();
        // After prospecting, tile is marked as prospected even if nothing found
        assert!(tile.is_prospected());
        assert_eq!(tile.resource_deposit(), None);
    }

    #[test]
    fn non_prospecting_terrain_is_always_prospected() {
        let tile = Tile::new(TerrainType::Grassland);
        assert!(tile.is_prospected());
    }

    #[test]
    fn grassland_set_resource_works() {
        let mut tile = Tile::new(TerrainType::Grassland);
        tile.set_resource(ResourceType::Grain);
        assert_eq!(tile.resource_deposit(), Some(ResourceType::Grain));
    }

    // ── Hidden resource visibility ──────────────────────────────

    #[test]
    fn hidden_deposit_via_set_resource_is_not_visible() {
        let mut tile = Tile::new(TerrainType::Hills);
        tile.set_resource(ResourceType::Coal);
        // Placed by map generator — not yet prospected
        assert!(!tile.is_prospected());
        assert!(!tile.has_visible_resource());
        assert_eq!(tile.resource_deposit(), Some(ResourceType::Coal));
    }

    #[test]
    fn hidden_deposit_becomes_visible_after_reveal() {
        let mut tile = Tile::new(TerrainType::Mountain);
        tile.set_resource(ResourceType::Gold); // generator places hidden
        assert!(!tile.has_visible_resource());
        tile.reveal_deposit(ResourceType::Gold); // prospector reveals
        assert!(tile.is_prospected());
        assert!(tile.has_visible_resource());
    }

    #[test]
    fn reveal_no_deposit_marks_prospected_no_resource() {
        let mut tile = Tile::new(TerrainType::Swamp);
        assert!(!tile.is_prospected());
        tile.reveal_no_deposit();
        assert!(tile.is_prospected());
        assert!(!tile.has_visible_resource());
        assert_eq!(tile.resource_deposit(), None);
    }

    #[test]
    fn surface_resource_always_visible() {
        let tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Grain);
        assert!(tile.has_visible_resource());
        assert!(tile.is_prospected()); // grassland can't have deposits
    }

    #[test]
    fn hills_with_wool_is_visible_not_undiscovered() {
        // Regression for adversarial-review F-002: Hills are deposit-capable
        // terrain (so `is_prospected()` returns the raw `prospected` flag, which
        // the generator leaves false) but Wool is a surface resource and is
        // already visible. AI undiscovered-tile logic must not flag this.
        let tile = Tile::with_resource(TerrainType::Hills, ResourceType::Wool);
        assert!(
            tile.terrain().can_have_deposits(),
            "Hills can have deposits"
        );
        assert!(
            !tile.is_prospected(),
            "fresh Hills tile is not yet prospected"
        );
        assert!(
            tile.has_visible_resource(),
            "Wool is a surface resource and is visible from turn 1"
        );
        // The canonical undiscovered predicate must therefore reject this tile.
        let is_undiscovered = tile.terrain().can_have_deposits()
            && !tile.is_prospected()
            && !tile.has_visible_resource();
        assert!(
            !is_undiscovered,
            "Hills + visible Wool must NOT be undiscovered"
        );
    }

    // ── Assigned civilian ──────────────────────────────────────

    #[test]
    fn assign_civilian() {
        let mut tile = Tile::new(TerrainType::Grassland);
        tile.assigned_civilian = Some(UnitId(7));
        assert_eq!(tile.assigned_civilian, Some(UnitId(7)));
    }

    #[test]
    fn clear_civilian() {
        let mut tile = Tile::new(TerrainType::Grassland);
        tile.assigned_civilian = Some(UnitId(7));
        tile.assigned_civilian = None;
        assert_eq!(tile.assigned_civilian, None);
    }

    // ── Capital flag ───────────────────────────────────────────

    #[test]
    fn set_capital() {
        let mut tile = Tile::new(TerrainType::Grassland);
        tile.is_capital = true;
        assert!(tile.is_capital);
    }

    // ── Yield: Tiles with resources at level 0 ──────────────────

    #[test]
    fn timber_tile_yields_1_timber() {
        let tile = Tile::with_resource(TerrainType::Forest, ResourceType::Timber);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Timber);
        assert_eq!(y.quantity, 1);
    }

    #[test]
    fn grain_tile_yields_1_grain() {
        let tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Grain);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Grain);
        assert_eq!(y.quantity, 1);
    }

    #[test]
    fn livestock_tile_yields_1_livestock() {
        let tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Livestock);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Livestock);
        assert_eq!(y.quantity, 1);
    }

    // ── Yield: Improvable resource tiles ──────────────────────

    #[test]
    fn grain_yield_scales_with_level() {
        let mut tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Grain);
        for expected_level in 0..=3u8 {
            let y = tile.calculate_yield().unwrap();
            assert_eq!(y.resource, ResourceType::Grain);
            assert_eq!(y.quantity, 1 + expected_level as u32);
            if expected_level < 3 {
                tile.improve();
            }
        }
    }

    #[test]
    fn fruit_yield_scales_with_level() {
        let mut tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Fruit);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Fruit, 1)
        );
        tile.set_improvement_level(1);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Fruit, 2)
        );
        tile.set_improvement_level(2);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Fruit, 3)
        );
        tile.set_improvement_level(3);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Fruit, 4)
        );
    }

    #[test]
    fn cotton_yield_scales_with_level() {
        let mut tile = Tile::with_resource(TerrainType::Grassland, ResourceType::Cotton);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Cotton, 1)
        );
        tile.set_improvement_level(3);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Cotton, 4)
        );
    }

    #[test]
    fn wool_yield_scales_with_level() {
        let mut tile = Tile::with_resource(TerrainType::Hills, ResourceType::Wool);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Wool, 1)
        );
        tile.set_improvement_level(2);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Wool, 3)
        );
    }

    #[test]
    fn timber_yield_scales_with_level() {
        let mut tile = Tile::with_resource(TerrainType::Forest, ResourceType::Timber);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Timber, 1)
        );
        tile.set_improvement_level(1);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Timber, 2)
        );
        tile.set_improvement_level(3);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Timber, 4)
        );
    }

    // ── Yield: Mining (Coal & Iron — 0/2/4/6) ──────────────────

    #[test]
    fn mountain_with_coal_table() {
        let mut tile = Tile::new(TerrainType::Mountain);
        tile.reveal_deposit(ResourceType::Coal);

        // Level 0: hidden mine produces nothing
        assert_eq!(tile.calculate_yield(), None);

        // Level 1: 2
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 2)
        );

        // Level 2: 4
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 4)
        );

        // Level 3: 6
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 6)
        );
    }

    #[test]
    fn hills_with_iron_table() {
        let mut tile = Tile::new(TerrainType::Hills);
        tile.reveal_deposit(ResourceType::Iron);

        assert_eq!(tile.calculate_yield(), None);

        tile.set_improvement_level(1);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 2)
        );

        tile.set_improvement_level(2);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 4)
        );

        tile.set_improvement_level(3);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 6)
        );
    }

    // ── Yield: Mining (Gold & Gems — special) ──────────────────

    #[test]
    fn mountain_with_gold_special_scaling() {
        let mut tile = Tile::new(TerrainType::Mountain);
        tile.reveal_deposit(ResourceType::Gold);

        // Level 0: nothing (needs mining first)
        assert_eq!(tile.calculate_yield(), None);

        // Level 1: 1 gold
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Gold, 1)
        );

        // Level 2: 2 gold
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Gold, 2)
        );
    }

    #[test]
    fn mountain_with_gems_special_scaling() {
        let mut tile = Tile::new(TerrainType::Mountain);
        tile.reveal_deposit(ResourceType::Gems);

        assert_eq!(tile.calculate_yield(), None);

        tile.set_improvement_level(1);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Gems, 1)
        );

        tile.set_improvement_level(2);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Gems, 2)
        );
    }

    #[test]
    fn mountain_with_gold_level_3() {
        let mut tile = Tile::new(TerrainType::Mountain);
        tile.reveal_deposit(ResourceType::Gold);
        tile.set_improvement_level(3);
        // Level 3: 3 gold (follows the pattern)
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Gold, 3)
        );
    }

    // ── Yield: No deposit on prospecting terrain ───────────────

    #[test]
    fn mountain_without_deposit_yields_nothing() {
        let tile = Tile::new(TerrainType::Mountain);
        assert_eq!(tile.calculate_yield(), None);
    }

    #[test]
    fn hills_without_deposit_yields_nothing() {
        let tile = Tile::new(TerrainType::Hills);
        assert_eq!(tile.calculate_yield(), None);
    }

    // ── Yield: Oil terrains ────────────────────────────────────

    #[test]
    fn swamp_with_oil_needs_improvement() {
        let mut tile = Tile::new(TerrainType::Swamp);
        tile.reveal_deposit(ResourceType::Oil);

        // Level 0: nothing (needs drilling infrastructure)
        assert_eq!(tile.calculate_yield(), None);

        // Level 1: 2
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 2)
        );

        // Level 2: 4
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 4)
        );

        // Level 3: 6
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 6)
        );
    }

    #[test]
    fn desert_with_oil() {
        let mut tile = Tile::new(TerrainType::Desert);
        tile.reveal_deposit(ResourceType::Oil);

        assert_eq!(tile.calculate_yield(), None);

        tile.set_improvement_level(2);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 4)
        );
    }

    #[test]
    fn tundra_with_oil() {
        let mut tile = Tile::new(TerrainType::Tundra);
        tile.reveal_deposit(ResourceType::Oil);

        assert_eq!(tile.calculate_yield(), None);

        tile.set_improvement_level(1);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 2)
        );
    }

    #[test]
    fn desert_without_deposit_yields_nothing() {
        let tile = Tile::new(TerrainType::Desert);
        assert_eq!(tile.calculate_yield(), None);
    }

    #[test]
    fn swamp_without_deposit_yields_nothing() {
        let tile = Tile::new(TerrainType::Swamp);
        assert_eq!(tile.calculate_yield(), None);
    }

    #[test]
    fn tundra_without_deposit_yields_nothing() {
        let tile = Tile::new(TerrainType::Tundra);
        assert_eq!(tile.calculate_yield(), None);
    }

    // ── Yield: Sea ─────────────────────────────────────────────

    #[test]
    fn sea_yields_nothing() {
        let tile = Tile::new(TerrainType::Sea);
        assert_eq!(tile.calculate_yield(), None);
    }

    // ── Comprehensive yield table test ─────────────────────────

    #[test]
    fn all_improvable_resources_at_all_levels() {
        // Verify that all improvable resource tiles follow +1 per level rule
        let cases: &[(TerrainType, ResourceType)] = &[
            (TerrainType::Grassland, ResourceType::Grain),
            (TerrainType::Grassland, ResourceType::Fruit),
            (TerrainType::Grassland, ResourceType::Cotton),
            (TerrainType::Hills, ResourceType::Wool),
            (TerrainType::Forest, ResourceType::Timber),
        ];

        for &(terrain, resource) in cases {
            let mut tile = Tile::with_resource(terrain, resource);
            for level in 0..=3u8 {
                tile.set_improvement_level(level);
                let y = tile.calculate_yield().unwrap();
                assert_eq!(
                    y.resource, resource,
                    "{terrain:?} with {resource:?} at level {level} should produce {resource:?}"
                );
                assert_eq!(
                    y.quantity,
                    1 + level as u32,
                    "{terrain:?} with {resource:?} at level {level} should produce {} but got {}",
                    1 + level as u32,
                    y.quantity
                );
            }
        }
    }

    #[test]
    fn coal_and_iron_table_all_levels() {
        let deposits = [ResourceType::Coal, ResourceType::Iron];
        let terrains = [TerrainType::Hills, TerrainType::Mountain];

        for terrain in terrains {
            for deposit in deposits {
                let mut tile = Tile::new(terrain);
                tile.reveal_deposit(deposit);

                tile.set_improvement_level(0);
                assert_eq!(
                    tile.calculate_yield(),
                    None,
                    "{terrain:?} with {deposit:?} at level 0 should yield nothing"
                );

                for level in 1..=3u8 {
                    tile.set_improvement_level(level);
                    let y = tile.calculate_yield().unwrap();
                    let expected = 2 * level as u32;
                    assert_eq!(
                        y.resource, deposit,
                        "{terrain:?} with {deposit:?} at level {level}"
                    );
                    assert_eq!(
                        y.quantity, expected,
                        "{terrain:?} with {deposit:?} at level {level}: expected {expected}, got {}",
                        y.quantity
                    );
                }
            }
        }
    }

    #[test]
    fn gold_and_gems_all_levels() {
        let deposits = [ResourceType::Gold, ResourceType::Gems];

        for deposit in deposits {
            let mut tile = Tile::new(TerrainType::Mountain);
            tile.reveal_deposit(deposit);

            // Level 0: no yield
            tile.set_improvement_level(0);
            assert_eq!(
                tile.calculate_yield(),
                None,
                "Mountain with {deposit:?} at level 0 should yield nothing"
            );

            // Levels 1-3: yield = level
            for level in 1..=3u8 {
                tile.set_improvement_level(level);
                let y = tile.calculate_yield().unwrap();
                assert_eq!(y.resource, deposit);
                assert_eq!(
                    y.quantity, level as u32,
                    "Mountain with {deposit:?} at level {level}: expected {level}, got {}",
                    y.quantity
                );
            }
        }
    }

    #[test]
    fn tile_without_resource_yields_nothing() {
        // Tiles without resources produce nothing regardless of terrain
        let terrains = [
            TerrainType::Grassland,
            TerrainType::Forest,
            TerrainType::Hills,
        ];

        for terrain in terrains {
            let tile = Tile::new(terrain);
            assert_eq!(
                tile.calculate_yield(),
                None,
                "{terrain:?} without resource should yield nothing"
            );
        }
    }
}
