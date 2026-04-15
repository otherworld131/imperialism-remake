use crate::hex::HexCoord;
use crate::map::UnitId;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique civilian UnitIds.
/// Range starts at 3_000_000 to avoid collision with army unit IDs (2_000_000+).
static CIVILIAN_ID_COUNTER: AtomicU32 = AtomicU32::new(3_000_000);

/// Generate a unique UnitId for a civilian unit.
pub fn next_civilian_id() -> UnitId {
    UnitId(CIVILIAN_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// The type of civilian specialist unit.
///
/// Each type can improve specific terrain types to increase resource output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CivilianType {
    /// Reveals hidden minerals on hills/mountains/swamp/desert/tundra.
    Prospector,
    /// Improves mines (barren hills, mountains).
    Miner,
    /// Builds railroads, depots, ports, forts.
    Engineer,
    /// Improves farms, orchards, plantations.
    Farmer,
    /// Improves fertile hills (wool), open range (livestock).
    Rancher,
    /// Improves hardwood forests.
    Forester,
    /// Improves oil wells (desert, swamp, tundra).
    Driller,
}

impl CivilianType {
    /// The cost in money to hire this civilian type.
    pub fn creation_cost(self) -> Money {
        match self {
            CivilianType::Prospector => Money::dollars(100),
            CivilianType::Miner => Money::dollars(1500),
            CivilianType::Engineer => Money::dollars(500),
            CivilianType::Farmer => Money::dollars(100),
            CivilianType::Rancher => Money::dollars(100),
            CivilianType::Forester => Money::dollars(100),
            CivilianType::Driller => Money::dollars(2000),
        }
    }

    /// Whether this civilian type can improve the given terrain/resource combination.
    pub fn can_improve(self, terrain: TerrainType, resource: Option<ResourceType>) -> bool {
        match self {
            Self::Farmer => matches!(
                resource,
                Some(ResourceType::Grain | ResourceType::Fruit | ResourceType::Cotton)
            ),
            Self::Rancher => matches!(
                resource,
                Some(ResourceType::Wool | ResourceType::Livestock | ResourceType::Horses)
            ),
            Self::Forester => matches!(resource, Some(ResourceType::Timber)),
            Self::Miner => matches!(
                resource,
                Some(
                    ResourceType::Coal
                        | ResourceType::Iron
                        | ResourceType::Gold
                        | ResourceType::Gems
                )
            ),
            Self::Driller => matches!(resource, Some(ResourceType::Oil)),
            Self::Prospector => terrain.can_have_deposits(),
            Self::Engineer => terrain.is_land(),
        }
    }

    /// The monetary cost of improving a tile, based on target improvement level.
    ///
    /// Level 1 costs $100, level 2+ costs $1000 each.
    pub fn improvement_cost(target_level: u8) -> Money {
        if target_level <= 1 {
            Money::dollars(100)
        } else {
            Money::dollars(1000)
        }
    }
}

impl std::fmt::Display for CivilianType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CivilianType::Prospector => write!(f, "Prospector"),
            CivilianType::Miner => write!(f, "Miner"),
            CivilianType::Engineer => write!(f, "Engineer"),
            CivilianType::Farmer => write!(f, "Farmer"),
            CivilianType::Rancher => write!(f, "Rancher"),
            CivilianType::Forester => write!(f, "Forester"),
            CivilianType::Driller => write!(f, "Driller"),
        }
    }
}

/// A civilian unit that can be deployed to improve terrain tiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Civilian {
    /// Unique identifier.
    pub id: UnitId,
    /// The type of civilian.
    pub civilian_type: CivilianType,
    /// The nation that owns this civilian.
    pub owner: NationId,
    /// Position on the hex map. `None` means the unit is in the capital, undeployed.
    pub position: Option<HexCoord>,
    /// Whether the civilian is currently working on improving a tile.
    pub working: bool,
    /// Turns remaining until the current improvement completes (0 = idle).
    pub turns_remaining: u8,
}

impl Civilian {
    /// Create a new civilian unit. Starts undeployed (position `None`), not working.
    pub fn new(id: UnitId, civilian_type: CivilianType, owner: NationId) -> Self {
        Self {
            id,
            civilian_type,
            owner,
            position: None,
            working: false,
            turns_remaining: 0,
        }
    }

    /// Deploy this civilian to a hex coordinate on the map.
    pub fn deploy(&mut self, coord: HexCoord) {
        self.position = Some(coord);
    }

    /// Begin working on an improvement that takes `turns` turns to complete.
    /// Does nothing if `turns` is 0 (prevents deadlock where tick() never completes).
    pub fn start_work(&mut self, turns: u8) {
        if turns == 0 {
            return;
        }
        self.working = true;
        self.turns_remaining = turns;
    }

    /// Advance the work timer by one turn.
    ///
    /// Returns `true` if work just completed this tick (transitions from
    /// `turns_remaining = 1` to `0` and sets `working = false`).
    pub fn tick(&mut self) -> bool {
        if !self.working || self.turns_remaining == 0 {
            return false;
        }
        self.turns_remaining -= 1;
        if self.turns_remaining == 0 {
            self.working = false;
            true
        } else {
            false
        }
    }
}

/// Parse a civilian type name string (case-insensitive).
pub fn parse_civilian_type(name: &str) -> Option<CivilianType> {
    match name.to_lowercase().as_str() {
        "prospector" => Some(CivilianType::Prospector),
        "miner" => Some(CivilianType::Miner),
        "engineer" => Some(CivilianType::Engineer),
        "farmer" => Some(CivilianType::Farmer),
        "rancher" => Some(CivilianType::Rancher),
        "forester" => Some(CivilianType::Forester),
        "driller" => Some(CivilianType::Driller),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_civilian_is_undeployed_and_idle() {
        let c = Civilian::new(UnitId(1), CivilianType::Farmer, NationId(0));
        assert_eq!(c.id, UnitId(1));
        assert_eq!(c.civilian_type, CivilianType::Farmer);
        assert_eq!(c.owner, NationId(0));
        assert_eq!(c.position, None);
        assert!(!c.working);
        assert_eq!(c.turns_remaining, 0);
    }

    // ── Creation costs ────────────────────────────────────────

    #[test]
    fn creation_costs() {
        assert_eq!(
            CivilianType::Prospector.creation_cost(),
            Money::dollars(100)
        );
        assert_eq!(CivilianType::Miner.creation_cost(), Money::dollars(1500));
        assert_eq!(CivilianType::Engineer.creation_cost(), Money::dollars(500));
        assert_eq!(CivilianType::Farmer.creation_cost(), Money::dollars(100));
        assert_eq!(CivilianType::Rancher.creation_cost(), Money::dollars(100));
        assert_eq!(CivilianType::Forester.creation_cost(), Money::dollars(100));
        assert_eq!(CivilianType::Driller.creation_cost(), Money::dollars(2000));
    }

    // ── Terrain matching ──────────────────────────────────────

    #[test]
    fn farmer_can_improve_agricultural_resources() {
        assert!(
            CivilianType::Farmer.can_improve(TerrainType::Grassland, Some(ResourceType::Grain))
        );
        assert!(
            CivilianType::Farmer.can_improve(TerrainType::Grassland, Some(ResourceType::Fruit))
        );
        assert!(
            CivilianType::Farmer.can_improve(TerrainType::Grassland, Some(ResourceType::Cotton))
        );
        assert!(!CivilianType::Farmer.can_improve(TerrainType::Mountain, Some(ResourceType::Coal)));
        assert!(!CivilianType::Farmer.can_improve(TerrainType::Sea, None));
    }

    #[test]
    fn rancher_can_improve_ranching_resources() {
        assert!(CivilianType::Rancher.can_improve(TerrainType::Hills, Some(ResourceType::Wool)));
        assert!(
            CivilianType::Rancher
                .can_improve(TerrainType::Grassland, Some(ResourceType::Livestock))
        );
        assert!(
            !CivilianType::Rancher.can_improve(TerrainType::Grassland, Some(ResourceType::Grain))
        );
        assert!(
            CivilianType::Rancher.can_improve(TerrainType::Grassland, Some(ResourceType::Horses))
        );
    }

    #[test]
    fn forester_can_improve_timber() {
        assert!(
            CivilianType::Forester.can_improve(TerrainType::Forest, Some(ResourceType::Timber))
        );
        assert!(!CivilianType::Forester.can_improve(TerrainType::Forest, None));
        assert!(
            !CivilianType::Forester.can_improve(TerrainType::Grassland, Some(ResourceType::Grain))
        );
    }

    #[test]
    fn miner_can_improve_mining_resources() {
        assert!(CivilianType::Miner.can_improve(TerrainType::Hills, Some(ResourceType::Coal)));
        assert!(CivilianType::Miner.can_improve(TerrainType::Mountain, Some(ResourceType::Iron)));
        assert!(!CivilianType::Miner.can_improve(TerrainType::Desert, Some(ResourceType::Oil)));
    }

    #[test]
    fn driller_can_improve_oil_resources() {
        assert!(CivilianType::Driller.can_improve(TerrainType::Desert, Some(ResourceType::Oil)));
        assert!(CivilianType::Driller.can_improve(TerrainType::Swamp, Some(ResourceType::Oil)));
        assert!(CivilianType::Driller.can_improve(TerrainType::Tundra, Some(ResourceType::Oil)));
        assert!(
            !CivilianType::Driller.can_improve(TerrainType::Mountain, Some(ResourceType::Coal))
        );
    }

    #[test]
    fn prospector_can_prospect_deposit_terrains() {
        assert!(CivilianType::Prospector.can_improve(TerrainType::Hills, None));
        assert!(CivilianType::Prospector.can_improve(TerrainType::Mountain, None));
        assert!(CivilianType::Prospector.can_improve(TerrainType::Swamp, None));
        assert!(CivilianType::Prospector.can_improve(TerrainType::Desert, None));
        assert!(CivilianType::Prospector.can_improve(TerrainType::Tundra, None));
        assert!(!CivilianType::Prospector.can_improve(TerrainType::Grassland, None));
    }

    #[test]
    fn engineer_can_work_on_any_land() {
        assert!(CivilianType::Engineer.can_improve(TerrainType::Grassland, None));
        assert!(CivilianType::Engineer.can_improve(TerrainType::Mountain, None));
        assert!(CivilianType::Engineer.can_improve(TerrainType::Desert, None));
        assert!(!CivilianType::Engineer.can_improve(TerrainType::Sea, None));
    }

    // ── Improvement costs ─────────────────────────────────────

    #[test]
    fn improvement_cost_level_1() {
        assert_eq!(CivilianType::improvement_cost(1), Money::dollars(100));
    }

    #[test]
    fn improvement_cost_level_2_and_above() {
        assert_eq!(CivilianType::improvement_cost(2), Money::dollars(1000));
        assert_eq!(CivilianType::improvement_cost(3), Money::dollars(1000));
    }

    // ── Deploy ────────────────────────────────────────────────

    #[test]
    fn deploy_sets_position() {
        let mut c = Civilian::new(UnitId(1), CivilianType::Farmer, NationId(0));
        assert_eq!(c.position, None);
        c.deploy(HexCoord::new(3, 4));
        assert_eq!(c.position, Some(HexCoord::new(3, 4)));
    }

    // ── Work cycle ────────────────────────────────────────────

    #[test]
    fn start_work_sets_state() {
        let mut c = Civilian::new(UnitId(1), CivilianType::Miner, NationId(0));
        c.start_work(2);
        assert!(c.working);
        assert_eq!(c.turns_remaining, 2);
    }

    #[test]
    fn tick_decrements_turns_remaining() {
        let mut c = Civilian::new(UnitId(1), CivilianType::Miner, NationId(0));
        c.start_work(3);

        // Tick 1: 3 -> 2, not done
        assert!(!c.tick());
        assert!(c.working);
        assert_eq!(c.turns_remaining, 2);

        // Tick 2: 2 -> 1, not done
        assert!(!c.tick());
        assert!(c.working);
        assert_eq!(c.turns_remaining, 1);

        // Tick 3: 1 -> 0, done!
        assert!(c.tick());
        assert!(!c.working);
        assert_eq!(c.turns_remaining, 0);
    }

    #[test]
    fn tick_single_turn_work_completes_immediately() {
        let mut c = Civilian::new(UnitId(1), CivilianType::Farmer, NationId(0));
        c.start_work(1);
        assert!(c.tick());
        assert!(!c.working);
        assert_eq!(c.turns_remaining, 0);
    }

    #[test]
    fn tick_when_not_working_returns_false() {
        let mut c = Civilian::new(UnitId(1), CivilianType::Farmer, NationId(0));
        assert!(!c.tick());
    }

    #[test]
    fn tick_after_completed_returns_false() {
        let mut c = Civilian::new(UnitId(1), CivilianType::Farmer, NationId(0));
        c.start_work(1);
        assert!(c.tick()); // completes
        assert!(!c.tick()); // already done
    }

    // ── Display ───────────────────────────────────────────────

    #[test]
    fn civilian_type_display() {
        assert_eq!(format!("{}", CivilianType::Prospector), "Prospector");
        assert_eq!(format!("{}", CivilianType::Farmer), "Farmer");
        assert_eq!(format!("{}", CivilianType::Driller), "Driller");
    }

    // ── Parse ─────────────────────────────────────────────────

    #[test]
    fn parse_civilian_type_valid() {
        assert_eq!(parse_civilian_type("farmer"), Some(CivilianType::Farmer));
        assert_eq!(parse_civilian_type("MINER"), Some(CivilianType::Miner));
        assert_eq!(
            parse_civilian_type("Prospector"),
            Some(CivilianType::Prospector)
        );
    }

    #[test]
    fn parse_civilian_type_invalid() {
        assert_eq!(parse_civilian_type("warrior"), None);
        assert_eq!(parse_civilian_type(""), None);
    }

    // ── Unique IDs ────────────────────────────────────────────

    #[test]
    fn unique_civilian_ids() {
        let id1 = next_civilian_id();
        let id2 = next_civilian_id();
        assert_ne!(id1, id2);
    }
}
