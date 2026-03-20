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
/// The terrain is immutable after creation. Resource deposits may be hidden
/// until revealed by prospecting. Improvement levels range from 0 (unimproved)
/// through 3 (fully developed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    /// The terrain type (immutable after creation).
    terrain: TerrainType,

    /// Hidden mineral deposit, revealed by prospecting.
    /// For terrains that `requires_prospecting()`, this starts as `None` until
    /// a prospector reveals it. For terrains that produce a base resource,
    /// this field is unused.
    resource_deposit: Option<ResourceType>,

    /// Improvement level: 0 (unimproved) through `terrain.max_improvement_level()`.
    improvement_level: u8,

    /// Infrastructure built on this tile.
    pub infrastructure: Infrastructure,

    /// The civilian unit currently working this tile, if any.
    pub assigned_civilian: Option<UnitId>,

    /// The province this tile belongs to.
    pub province_id: Option<ProvinceId>,

    /// Whether this tile is the capital of its province.
    pub is_capital: bool,
}

impl Tile {
    /// Create a new tile with the given terrain type. Starts unimproved with no infrastructure.
    pub fn new(terrain: TerrainType) -> Self {
        Self {
            terrain,
            resource_deposit: None,
            improvement_level: 0,
            infrastructure: Infrastructure::NONE,
            assigned_civilian: None,
            province_id: None,
            is_capital: false,
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

    /// The immutable terrain type of this tile.
    pub fn terrain(&self) -> TerrainType {
        self.terrain
    }

    /// The hidden resource deposit (if any). `None` means either no deposit
    /// exists or it has not yet been revealed by prospecting.
    pub fn resource_deposit(&self) -> Option<ResourceType> {
        self.resource_deposit
    }

    /// Current improvement level (0-3).
    pub fn improvement_level(&self) -> u8 {
        self.improvement_level
    }

    // ── Resource deposit ───────────────────────────────────────

    /// Reveal a hidden mineral deposit via prospecting.
    ///
    /// Only valid for terrains that require prospecting. Panics if the terrain
    /// does not support prospecting.
    pub fn reveal_deposit(&mut self, resource: ResourceType) {
        assert!(
            self.terrain.requires_prospecting(),
            "Cannot reveal deposit on {:?} — terrain does not require prospecting",
            self.terrain
        );
        self.resource_deposit = Some(resource);
    }

    /// Set the deposit to `None` (e.g., prospecting found nothing).
    ///
    /// Only valid for terrains that require prospecting.
    pub fn reveal_no_deposit(&mut self) {
        assert!(
            self.terrain.requires_prospecting(),
            "Cannot clear deposit on {:?} — terrain does not require prospecting",
            self.terrain
        );
        self.resource_deposit = None;
    }

    /// Whether this tile has been prospected (deposit revealed or confirmed empty).
    /// For non-prospecting terrains, always returns `true` (nothing to prospect).
    pub fn is_prospected(&self) -> bool {
        if self.terrain.requires_prospecting() {
            // A prospected tile either has a revealed deposit or has been
            // explicitly checked. We track "prospected" by improvement_level > 0
            // or deposit being Some. However, for simplicity the caller should
            // track prospecting state externally. Here we only report whether
            // a deposit is known.
            self.resource_deposit.is_some()
        } else {
            true
        }
    }

    // ── Improvement ────────────────────────────────────────────

    /// Attempt to improve this tile by one level.
    ///
    /// Returns `true` if the improvement succeeded, `false` if already at max level
    /// or the terrain cannot be improved.
    pub fn improve(&mut self) -> bool {
        let max = self.terrain.max_improvement_level();
        if self.improvement_level < max {
            self.improvement_level += 1;
            true
        } else {
            false
        }
    }

    /// Set the improvement level directly. Clamps to the terrain's maximum.
    pub fn set_improvement_level(&mut self, level: u8) {
        self.improvement_level = level.min(self.terrain.max_improvement_level());
    }

    // ── Yield calculation ──────────────────────────────────────

    /// Calculate the resource yield of this tile based on terrain, improvement level,
    /// and any revealed resource deposit.
    ///
    /// Returns `None` if the tile produces nothing (e.g., unprospected mountain,
    /// sea tile, desert with no deposit).
    ///
    /// ## Yield Rules
    ///
    /// - **ScrubForest**: always 1 timber, cannot be improved.
    /// - **DryPlains**: always 1 grain, cannot be improved.
    /// - **HorseRanch**: always 1 horse.
    /// - **OpenRange**: always 1 livestock.
    /// - **Farm, Orchard, Plantation, FertileHills, HardwoodForest**: base resource,
    ///   +1 per improvement level (so level 0 = 1, level 3 = 4).
    /// - **BarrenHills / Mountain with Coal or Iron**: +2 per improvement level
    ///   (level 0 = 1, level 1 = 3, level 2 = 5, level 3 = 7). Double rate for mines.
    /// - **Mountain with Gold or Gems**: 1 at level 1, 2 at level 2. Requires prospecting.
    /// - **Swamp / Desert / Tundra with Oil**: +1 per improvement level (level 1 = 1, etc.).
    ///   Oil requires at least level 1 to produce anything.
    pub fn calculate_yield(&self) -> Option<ResourceAmount> {
        match self.terrain {
            // Fixed-output terrains (no improvement possible)
            TerrainType::ScrubForest => Some(ResourceAmount::new(ResourceType::Timber, 1)),
            TerrainType::DryPlains => Some(ResourceAmount::new(ResourceType::Grain, 1)),
            TerrainType::HorseRanch => Some(ResourceAmount::new(ResourceType::Horses, 1)),
            TerrainType::OpenRange => Some(ResourceAmount::new(ResourceType::Livestock, 1)),

            // Improvable agricultural/forestry terrains: base 1, +1 per level
            TerrainType::Farm => {
                let qty = 1 + self.improvement_level as u32;
                Some(ResourceAmount::new(ResourceType::Grain, qty))
            }
            TerrainType::Orchard => {
                let qty = 1 + self.improvement_level as u32;
                Some(ResourceAmount::new(ResourceType::Fruit, qty))
            }
            TerrainType::Plantation => {
                let qty = 1 + self.improvement_level as u32;
                Some(ResourceAmount::new(ResourceType::Cotton, qty))
            }
            TerrainType::FertileHills => {
                let qty = 1 + self.improvement_level as u32;
                Some(ResourceAmount::new(ResourceType::Wool, qty))
            }
            TerrainType::HardwoodForest => {
                let qty = 1 + self.improvement_level as u32;
                Some(ResourceAmount::new(ResourceType::Timber, qty))
            }

            // Mining terrains: require prospecting
            TerrainType::BarrenHills | TerrainType::Mountain => self.calculate_mining_yield(),

            // Oil terrains: require prospecting + improvement
            TerrainType::Swamp | TerrainType::Desert | TerrainType::Tundra => {
                self.calculate_oil_yield()
            }

            // Sea produces nothing
            TerrainType::Sea => None,
        }
    }

    /// Calculate yield for BarrenHills / Mountain with a revealed deposit.
    fn calculate_mining_yield(&self) -> Option<ResourceAmount> {
        let deposit = self.resource_deposit?;

        match deposit {
            // Coal and Iron: double rate (+2 per improvement level)
            ResourceType::Coal | ResourceType::Iron => {
                let qty = 1 + 2 * self.improvement_level as u32;
                Some(ResourceAmount::new(deposit, qty))
            }

            // Gold and Gems: special scaling (1 at level 1, 2 at level 2)
            ResourceType::Gold | ResourceType::Gems => {
                if self.improvement_level == 0 {
                    None
                } else {
                    let qty = self.improvement_level as u32;
                    Some(ResourceAmount::new(deposit, qty))
                }
            }

            // Other deposit types on mountains/hills are unexpected but handled gracefully
            _ => {
                let qty = 1 + self.improvement_level as u32;
                Some(ResourceAmount::new(deposit, qty))
            }
        }
    }

    /// Calculate yield for Swamp / Desert / Tundra with oil deposit.
    fn calculate_oil_yield(&self) -> Option<ResourceAmount> {
        let deposit = self.resource_deposit?;

        // Oil requires at least level 1 to produce
        if self.improvement_level == 0 {
            return None;
        }

        let qty = self.improvement_level as u32;
        Some(ResourceAmount::new(deposit, qty))
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_tile_is_unimproved() {
        let tile = Tile::new(TerrainType::Farm);
        assert_eq!(tile.terrain(), TerrainType::Farm);
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
        let tile = Tile::new(TerrainType::Farm);
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
        let mut tile = Tile::new(TerrainType::Farm);
        tile.infrastructure.has_railroad = true;
        tile.infrastructure.has_depot = true;
        assert!(tile.infrastructure.has_railroad);
        assert!(tile.infrastructure.has_depot);
    }

    #[test]
    fn infrastructure_fort_with_level() {
        let mut tile = Tile::new(TerrainType::FertileHills);
        tile.infrastructure.has_fort = true;
        tile.infrastructure.fort_level = 2;
        assert!(tile.infrastructure.has_fort);
        assert_eq!(tile.infrastructure.fort_level, 2);
    }

    // ── Improvement ────────────────────────────────────────────

    #[test]
    fn improve_farm_increases_level() {
        let mut tile = Tile::new(TerrainType::Farm);
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
    fn improve_scrub_forest_fails() {
        let mut tile = Tile::new(TerrainType::ScrubForest);
        assert!(!tile.improve());
        assert_eq!(tile.improvement_level(), 0);
    }

    #[test]
    fn improve_dry_plains_fails() {
        let mut tile = Tile::new(TerrainType::DryPlains);
        assert!(!tile.improve());
    }

    #[test]
    fn improve_open_range_fails() {
        let mut tile = Tile::new(TerrainType::OpenRange);
        assert!(!tile.improve());
    }

    #[test]
    fn improve_horse_ranch_fails() {
        let mut tile = Tile::new(TerrainType::HorseRanch);
        assert!(!tile.improve());
    }

    #[test]
    fn improve_sea_fails() {
        let mut tile = Tile::new(TerrainType::Sea);
        assert!(!tile.improve());
    }

    #[test]
    fn set_improvement_level_clamps_to_max() {
        let mut tile = Tile::new(TerrainType::Farm);
        tile.set_improvement_level(10);
        assert_eq!(tile.improvement_level(), 3); // Farm max is 3
    }

    #[test]
    fn set_improvement_level_on_non_improvable() {
        let mut tile = Tile::new(TerrainType::ScrubForest);
        tile.set_improvement_level(5);
        assert_eq!(tile.improvement_level(), 0); // max is 0
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
    fn reveal_deposit_on_barren_hills() {
        let mut tile = Tile::new(TerrainType::BarrenHills);
        tile.reveal_deposit(ResourceType::Coal);
        assert_eq!(tile.resource_deposit(), Some(ResourceType::Coal));
    }

    #[test]
    fn reveal_no_deposit() {
        let mut tile = Tile::new(TerrainType::Desert);
        tile.reveal_no_deposit();
        assert!(!tile.is_prospected());
        assert_eq!(tile.resource_deposit(), None);
    }

    #[test]
    fn non_prospecting_terrain_is_always_prospected() {
        let tile = Tile::new(TerrainType::Farm);
        assert!(tile.is_prospected());
    }

    #[test]
    #[should_panic(expected = "does not require prospecting")]
    fn reveal_deposit_on_farm_panics() {
        let mut tile = Tile::new(TerrainType::Farm);
        tile.reveal_deposit(ResourceType::Coal);
    }

    #[test]
    #[should_panic(expected = "does not require prospecting")]
    fn reveal_no_deposit_on_farm_panics() {
        let mut tile = Tile::new(TerrainType::Farm);
        tile.reveal_no_deposit();
    }

    // ── Assigned civilian ──────────────────────────────────────

    #[test]
    fn assign_civilian() {
        let mut tile = Tile::new(TerrainType::Farm);
        tile.assigned_civilian = Some(UnitId(7));
        assert_eq!(tile.assigned_civilian, Some(UnitId(7)));
    }

    #[test]
    fn clear_civilian() {
        let mut tile = Tile::new(TerrainType::Farm);
        tile.assigned_civilian = Some(UnitId(7));
        tile.assigned_civilian = None;
        assert_eq!(tile.assigned_civilian, None);
    }

    // ── Capital flag ───────────────────────────────────────────

    #[test]
    fn set_capital() {
        let mut tile = Tile::new(TerrainType::Farm);
        tile.is_capital = true;
        assert!(tile.is_capital);
    }

    // ── Yield: Fixed-output terrains ───────────────────────────

    #[test]
    fn scrub_forest_always_1_timber() {
        let tile = Tile::new(TerrainType::ScrubForest);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Timber);
        assert_eq!(y.quantity, 1);
    }

    #[test]
    fn dry_plains_always_1_grain() {
        let tile = Tile::new(TerrainType::DryPlains);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Grain);
        assert_eq!(y.quantity, 1);
    }

    #[test]
    fn horse_ranch_always_1_horse() {
        let tile = Tile::new(TerrainType::HorseRanch);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Horses);
        assert_eq!(y.quantity, 1);
    }

    #[test]
    fn open_range_always_1_livestock() {
        let tile = Tile::new(TerrainType::OpenRange);
        let y = tile.calculate_yield().unwrap();
        assert_eq!(y.resource, ResourceType::Livestock);
        assert_eq!(y.quantity, 1);
    }

    // ── Yield: Improvable agricultural terrains ────────────────

    #[test]
    fn farm_yield_scales_with_level() {
        let mut tile = Tile::new(TerrainType::Farm);
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
    fn orchard_yield_scales_with_level() {
        let mut tile = Tile::new(TerrainType::Orchard);
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
    fn plantation_yield_scales_with_level() {
        let mut tile = Tile::new(TerrainType::Plantation);
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
    fn fertile_hills_yield_scales_with_level() {
        let mut tile = Tile::new(TerrainType::FertileHills);
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
    fn hardwood_forest_yield_scales_with_level() {
        let mut tile = Tile::new(TerrainType::HardwoodForest);
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

    // ── Yield: Mining (Coal & Iron — double rate) ──────────────

    #[test]
    fn mountain_with_coal_double_rate() {
        let mut tile = Tile::new(TerrainType::Mountain);
        tile.reveal_deposit(ResourceType::Coal);

        // Level 0: base 1
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 1)
        );

        // Level 1: 1 + 2*1 = 3
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 3)
        );

        // Level 2: 1 + 2*2 = 5
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 5)
        );

        // Level 3: 1 + 2*3 = 7
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Coal, 7)
        );
    }

    #[test]
    fn barren_hills_with_iron_double_rate() {
        let mut tile = Tile::new(TerrainType::BarrenHills);
        tile.reveal_deposit(ResourceType::Iron);

        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 1)
        );

        tile.set_improvement_level(1);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 3)
        );

        tile.set_improvement_level(2);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 5)
        );

        tile.set_improvement_level(3);
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Iron, 7)
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
    fn barren_hills_without_deposit_yields_nothing() {
        let tile = Tile::new(TerrainType::BarrenHills);
        assert_eq!(tile.calculate_yield(), None);
    }

    // ── Yield: Oil terrains ────────────────────────────────────

    #[test]
    fn swamp_with_oil_needs_improvement() {
        let mut tile = Tile::new(TerrainType::Swamp);
        tile.reveal_deposit(ResourceType::Oil);

        // Level 0: nothing (needs drilling infrastructure)
        assert_eq!(tile.calculate_yield(), None);

        // Level 1: 1 oil
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 1)
        );

        // Level 2: 2 oil
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 2)
        );

        // Level 3: 3 oil
        tile.improve();
        assert_eq!(
            tile.calculate_yield().unwrap(),
            ResourceAmount::new(ResourceType::Oil, 3)
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
            ResourceAmount::new(ResourceType::Oil, 2)
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
            ResourceAmount::new(ResourceType::Oil, 1)
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
    fn all_improvable_terrains_at_all_levels() {
        // Verify that all improvable terrains follow +1 per level rule
        let cases: &[(TerrainType, ResourceType)] = &[
            (TerrainType::Farm, ResourceType::Grain),
            (TerrainType::Orchard, ResourceType::Fruit),
            (TerrainType::Plantation, ResourceType::Cotton),
            (TerrainType::FertileHills, ResourceType::Wool),
            (TerrainType::HardwoodForest, ResourceType::Timber),
        ];

        for &(terrain, resource) in cases {
            let mut tile = Tile::new(terrain);
            for level in 0..=3u8 {
                tile.set_improvement_level(level);
                let y = tile.calculate_yield().unwrap();
                assert_eq!(
                    y.resource, resource,
                    "{terrain:?} at level {level} should produce {resource:?}"
                );
                assert_eq!(
                    y.quantity,
                    1 + level as u32,
                    "{terrain:?} at level {level} should produce {} but got {}",
                    1 + level as u32,
                    y.quantity
                );
            }
        }
    }

    #[test]
    fn coal_and_iron_double_rate_all_levels() {
        let deposits = [ResourceType::Coal, ResourceType::Iron];
        let terrains = [TerrainType::BarrenHills, TerrainType::Mountain];

        for terrain in terrains {
            for deposit in deposits {
                let mut tile = Tile::new(terrain);
                tile.reveal_deposit(deposit);

                for level in 0..=3u8 {
                    tile.set_improvement_level(level);
                    let y = tile.calculate_yield().unwrap();
                    let expected = 1 + 2 * level as u32;
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
    fn fixed_output_terrains_unaffected_by_level_attempt() {
        // These terrains cannot be improved, so their output is constant
        let cases = [
            (TerrainType::ScrubForest, ResourceType::Timber, 1),
            (TerrainType::DryPlains, ResourceType::Grain, 1),
            (TerrainType::HorseRanch, ResourceType::Horses, 1),
            (TerrainType::OpenRange, ResourceType::Livestock, 1),
        ];

        for (terrain, resource, qty) in cases {
            let mut tile = Tile::new(terrain);
            // Attempt to improve (should fail)
            tile.improve();
            let y = tile.calculate_yield().unwrap();
            assert_eq!(y.resource, resource);
            assert_eq!(y.quantity, qty);
        }
    }
}
