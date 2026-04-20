use crate::hex::HexCoord;
use crate::map::hex_map::HexMap;
use crate::types::*;

/// The level of industrialization of a province's settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SettlementLevel {
    /// Smallest settlement — starting state for newly connected provinces.
    Hamlet,
    /// Mid-tier settlement — unlocks additional production.
    Village,
    /// Largest settlement — full industrial capacity.
    Town,
}

/// A province: a contiguous group of hex tiles owned by a nation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Province {
    /// Unique identifier.
    pub id: ProvinceId,
    /// Display name (e.g., "Bavaria", "London City").
    pub name: String,
    /// The nation that owns this province.
    pub owner: NationId,
    /// The capital tile of this province.
    pub capital_tile: HexCoord,
    /// All tiles belonging to the province (includes the capital tile).
    pub tiles: Vec<HexCoord>,
    /// Number of immovable garrison units (Militia/Minutemen).
    /// Great Powers get 4, Minor Nations get 3.
    pub garrison_count: u8,
    /// Current industrialization level of the province settlement.
    pub settlement_level: SettlementLevel,
    /// Whether this province is connected to the nation's capital via
    /// an unbroken chain of depots / ports.
    pub connected_to_capital: bool,
    /// Turns remaining until industrialization completes.
    /// `Some(n)` means industrialization is in progress with `n` turns left.
    /// Starts at 6 when a province first becomes connected.
    /// `None` means either not yet connected or already industrialized.
    pub industrialization_turns_remaining: Option<u8>,
    /// Turns remaining until this Village upgrades to Town.
    /// `Some(n)` means upgrade in progress; starts at 12 when a province
    /// first reaches Village status while connected.
    /// `None` means not yet a Village or already a Town.
    #[serde(default)]
    pub town_countdown: Option<u8>,
    /// Whether this province has at least one tile adjacent to a Sea tile.
    /// Computed at map generation time. Coastal provinces can be targeted
    /// by naval landings (beachhead operations).
    #[serde(default)]
    pub coastal: bool,
    /// If this province was diplomatically incorporated from a minor nation,
    /// tracks the original minor nation's ID. Used for map rendering only:
    /// incorporated provinces keep separate borders and show a lighter shade
    /// of the overlord's color. `None` for native GP provinces, independent
    /// minor provinces, and militarily conquered provinces.
    #[serde(default)]
    pub incorporated_from: Option<NationId>,
    /// If this province was originally owned by a minor nation that later
    /// lost it purely through military conquest (no diplomatic integration),
    /// tracks that original minor. Purely mechanical — used by card #79 to
    /// restore the province when the current overlord falls into anarchy.
    /// Does NOT affect map rendering (unlike `incorporated_from`).
    #[serde(default)]
    pub conquest_origin: Option<NationId>,
}

impl Province {
    /// Create a new province.
    ///
    /// # Arguments
    /// * `id` — unique province identifier
    /// * `name` — display name
    /// * `owner` — owning nation
    /// * `capital_tile` — the hex position of the province capital
    /// * `tiles` — all hex tiles in the province (should include `capital_tile`)
    /// * `garrison_count` — number of garrison units (4 for Great Powers, 3 for Minor Nations)
    pub fn new(
        id: ProvinceId,
        name: String,
        owner: NationId,
        capital_tile: HexCoord,
        tiles: Vec<HexCoord>,
        garrison_count: u8,
    ) -> Self {
        Self {
            id,
            name,
            owner,
            capital_tile,
            tiles,
            garrison_count,
            settlement_level: SettlementLevel::Hamlet,
            connected_to_capital: false,
            industrialization_turns_remaining: None,
            town_countdown: None,
            coastal: false,
            incorporated_from: None,
            conquest_origin: None,
        }
    }

    /// Build the capital-city name for a national capital.
    ///
    /// Convention: the province that contains a nation's capital is named
    /// `"{nation_name} City"` (e.g., "France City", "Britain City").
    pub fn capital_city_name(nation_name: &str) -> String {
        format!("{nation_name} City")
    }

    /// Whether this province can autonomously produce materials and goods.
    ///
    /// Only Village and Town settlements produce autonomously.
    pub fn can_produce(&self) -> bool {
        matches!(
            self.settlement_level,
            SettlementLevel::Village | SettlementLevel::Town
        )
    }

    /// Whether this province contains any coastal tile (adjacent to Sea).
    ///
    /// Computed at map generation time and stored in the `coastal` field.
    pub fn is_coastal(&self) -> bool {
        self.coastal
    }

    /// The number of tiles in this province.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
}

/// Returns true if two provinces share at least one hex edge
/// (a tile in `prov_a` has a neighbor tile that belongs to `prov_b`).
pub fn provinces_are_adjacent(hex_map: &HexMap, prov_a: &Province, prov_b: &Province) -> bool {
    for tile_coord in &prov_a.tiles {
        for neighbor in tile_coord.neighbors() {
            if let Some(tile) = hex_map.get_tile(neighbor)
                && tile.province_id == Some(prov_b.id)
            {
                return true;
            }
        }
    }
    false
}

/// Compute whether a province has any tile adjacent to a Sea tile on the given map.
pub fn compute_coastal(hex_map: &HexMap, province: &Province) -> bool {
    for tile_coord in &province.tiles {
        for neighbor in tile_coord.neighbors() {
            if let Some(tile) = hex_map.get_tile(neighbor)
                && tile.terrain() == TerrainType::Sea
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a simple province for testing.
    fn sample_province() -> Province {
        let capital = HexCoord::new(0, 0);
        let tiles = vec![
            capital,
            HexCoord::new(1, 0),
            HexCoord::new(0, 1),
            HexCoord::new(-1, 1),
        ];
        Province::new(
            ProvinceId(1),
            "Bavaria".to_string(),
            NationId(1),
            capital,
            tiles,
            4,
        )
    }

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_province_has_correct_fields() {
        let p = sample_province();
        assert_eq!(p.id, ProvinceId(1));
        assert_eq!(p.name, "Bavaria");
        assert_eq!(p.owner, NationId(1));
        assert_eq!(p.capital_tile, HexCoord::new(0, 0));
        assert_eq!(p.garrison_count, 4);
    }

    #[test]
    fn new_province_defaults() {
        let p = sample_province();
        assert_eq!(p.settlement_level, SettlementLevel::Hamlet);
        assert!(!p.connected_to_capital);
        assert_eq!(p.industrialization_turns_remaining, None);
    }

    // ── tile_count ─────────────────────────────────────────────

    #[test]
    fn tile_count_matches_tiles_vec() {
        let p = sample_province();
        assert_eq!(p.tile_count(), 4);
    }

    #[test]
    fn tile_count_single_tile() {
        let capital = HexCoord::new(5, -3);
        let p = Province::new(
            ProvinceId(99),
            "Tiny".to_string(),
            NationId(2),
            capital,
            vec![capital],
            3,
        );
        assert_eq!(p.tile_count(), 1);
    }

    // ── is_coastal ─────────────────────────────────────────────

    #[test]
    fn is_coastal_defaults_to_false() {
        let p = sample_province();
        assert!(!p.is_coastal());
    }

    #[test]
    fn is_coastal_returns_true_when_set() {
        let mut p = sample_province();
        p.coastal = true;
        assert!(p.is_coastal());
    }

    // ── capital_city_name ──────────────────────────────────────

    #[test]
    fn capital_city_name_appends_city() {
        assert_eq!(Province::capital_city_name("France"), "France City");
        assert_eq!(Province::capital_city_name("Britain"), "Britain City");
    }

    // ── garrison counts ────────────────────────────────────────

    #[test]
    fn great_power_garrison() {
        let p = sample_province(); // garrison_count = 4
        assert_eq!(p.garrison_count, 4);
    }

    #[test]
    fn minor_nation_garrison() {
        let capital = HexCoord::new(2, 2);
        let p = Province::new(
            ProvinceId(10),
            "Minor Land".to_string(),
            NationId(5),
            capital,
            vec![capital, HexCoord::new(3, 2)],
            3,
        );
        assert_eq!(p.garrison_count, 3);
    }

    // ── settlement level ───────────────────────────────────────

    #[test]
    fn settlement_level_starts_at_hamlet() {
        let p = sample_province();
        assert_eq!(p.settlement_level, SettlementLevel::Hamlet);
    }

    #[test]
    fn settlement_level_can_be_changed() {
        let mut p = sample_province();
        p.settlement_level = SettlementLevel::Village;
        assert_eq!(p.settlement_level, SettlementLevel::Village);
        p.settlement_level = SettlementLevel::Town;
        assert_eq!(p.settlement_level, SettlementLevel::Town);
    }

    // ── can_produce ─────────────────────────────────────────────

    #[test]
    fn hamlet_cannot_produce() {
        let p = sample_province();
        assert_eq!(p.settlement_level, SettlementLevel::Hamlet);
        assert!(!p.can_produce());
    }

    #[test]
    fn village_can_produce() {
        let mut p = sample_province();
        p.settlement_level = SettlementLevel::Village;
        assert!(p.can_produce());
    }

    #[test]
    fn town_can_produce() {
        let mut p = sample_province();
        p.settlement_level = SettlementLevel::Town;
        assert!(p.can_produce());
    }

    // ── town_countdown ──────────────────────────────────────────

    #[test]
    fn town_countdown_defaults_to_none() {
        let p = sample_province();
        assert_eq!(p.town_countdown, None);
    }

    #[test]
    fn town_countdown_can_be_set() {
        let mut p = sample_province();
        p.town_countdown = Some(12);
        assert_eq!(p.town_countdown, Some(12));
    }

    // ── connectivity & industrialization ────────────────────────

    #[test]
    fn initially_not_connected() {
        let p = sample_province();
        assert!(!p.connected_to_capital);
    }

    #[test]
    fn industrialization_begins_on_connection() {
        let mut p = sample_province();
        // Simulate connecting to capital
        p.connected_to_capital = true;
        p.industrialization_turns_remaining = Some(6);
        assert_eq!(p.industrialization_turns_remaining, Some(6));
    }

    #[test]
    fn industrialization_countdown() {
        let mut p = sample_province();
        p.connected_to_capital = true;
        p.industrialization_turns_remaining = Some(6);

        // Simulate ticking down
        for expected in (0..6).rev() {
            let remaining = p.industrialization_turns_remaining.unwrap();
            p.industrialization_turns_remaining = if remaining > 0 {
                Some(remaining - 1)
            } else {
                None
            };
            if expected > 0 {
                assert_eq!(p.industrialization_turns_remaining, Some(expected));
            }
        }
        // After reaching 0, set to None (industrialized)
        p.industrialization_turns_remaining = None;
        assert_eq!(p.industrialization_turns_remaining, None);
    }

    // ── provinces_are_adjacent ──────────────────────────────────

    #[test]
    fn adjacent_provinces_detected() {
        use crate::map::tile::Tile;

        let mut hex_map = crate::map::HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov_a = Province::new(
            ProvinceId(1),
            "A".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov_b = Province::new(
            ProvinceId(2),
            "B".into(),
            NationId(2),
            HexCoord::new(1, 0),
            vec![HexCoord::new(1, 0)],
            3,
        );

        assert!(provinces_are_adjacent(&hex_map, &prov_a, &prov_b));
        assert!(provinces_are_adjacent(&hex_map, &prov_b, &prov_a));
    }

    #[test]
    fn non_adjacent_provinces_not_detected() {
        use crate::map::tile::Tile;

        let mut hex_map = crate::map::HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(5, 5),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov_a = Province::new(
            ProvinceId(1),
            "A".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );
        let prov_b = Province::new(
            ProvinceId(2),
            "B".into(),
            NationId(2),
            HexCoord::new(5, 5),
            vec![HexCoord::new(5, 5)],
            3,
        );

        assert!(!provinces_are_adjacent(&hex_map, &prov_a, &prov_b));
    }

    // ── compute_coastal ─────────────────────────────────────────

    #[test]
    fn compute_coastal_true_when_sea_neighbor() {
        use crate::map::tile::Tile;

        let mut hex_map = crate::map::HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(HexCoord::new(1, 0), Tile::new(TerrainType::Sea));

        let prov = Province::new(
            ProvinceId(1),
            "Coastal".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );

        assert!(compute_coastal(&hex_map, &prov));
    }

    #[test]
    fn compute_coastal_false_when_no_sea_neighbor() {
        use crate::map::tile::Tile;

        let mut hex_map = crate::map::HexMap::new(10, 10);
        hex_map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        hex_map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let prov = Province::new(
            ProvinceId(1),
            "Inland".into(),
            NationId(1),
            HexCoord::new(0, 0),
            vec![HexCoord::new(0, 0)],
            4,
        );

        assert!(!compute_coastal(&hex_map, &prov));
    }
}
