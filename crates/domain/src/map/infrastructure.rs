use crate::data::{GameConfig, GameData};
use crate::hex::HexCoord;
use crate::map::hex_map::HexMap;
use crate::map::province::Province;
use crate::nation::Nation;
use crate::types::*;
use crate::DomainError;
use std::collections::{HashSet, VecDeque};

/// Name of the tech (from Lua `game_config`) that gates railroad construction
/// on the given terrain. `None` means the terrain is always rail-buildable
/// (sea returns `None` because sea can never be rail-built regardless).
pub fn railroad_required_tech(terrain: TerrainType, cfg: &GameConfig) -> Option<&str> {
    let name: &Option<String> = match terrain {
        TerrainType::Grassland => &cfg.railroad_tech_grassland,
        TerrainType::Forest => &cfg.railroad_tech_forest,
        TerrainType::Desert => &cfg.railroad_tech_desert,
        TerrainType::Tundra => &cfg.railroad_tech_tundra,
        TerrainType::Hills => &cfg.railroad_tech_hills,
        TerrainType::Swamp => &cfg.railroad_tech_swamp,
        TerrainType::Mountain => &cfg.railroad_tech_mountain,
        TerrainType::Sea => return None,
    };
    name.as_deref()
}

/// True if a nation with `researched_techs` has the tech required to lay
/// railroad on `terrain`. Sea always returns false (not rail-buildable).
pub fn rail_terrain_enabled(
    terrain: TerrainType,
    researched_techs: &[crate::events::TechId],
    game_data: &GameData,
    cfg: &GameConfig,
) -> bool {
    if terrain == TerrainType::Sea {
        return false;
    }
    match railroad_required_tech(terrain, cfg) {
        None => true,
        Some(tech_name) => game_data
            .tech_tree
            .get_by_name(tech_name)
            .is_some_and(|t| researched_techs.contains(&t.id)),
    }
}

/// Convenience wrapper: look up `nation.researched_techs` and delegate.
pub fn rail_terrain_enabled_for(
    terrain: TerrainType,
    nation: &Nation,
    game_data: &GameData,
    cfg: &GameConfig,
) -> bool {
    rail_terrain_enabled(terrain, &nation.researched_techs, game_data, cfg)
}

/// Cost to build a railroad on a terrain type.
/// Returns `None` for terrain where railroads cannot be built (Sea).
pub fn railroad_cost(terrain: TerrainType, cfg: &GameConfig) -> Option<Money> {
    let dollars = match terrain {
        TerrainType::Grassland => cfg.railroad_cost_grassland,
        TerrainType::Forest => cfg.railroad_cost_forest,
        TerrainType::Desert => cfg.railroad_cost_desert,
        TerrainType::Tundra => cfg.railroad_cost_tundra,
        TerrainType::Swamp => cfg.railroad_cost_swamp,
        TerrainType::Hills => cfg.railroad_cost_hills,
        TerrainType::Mountain => cfg.railroad_cost_mountain,
        TerrainType::Sea => return None,
    };
    Some(Money::dollars(dollars))
}

/// True if the tile exists, has a province, and that province is owned by `nation_id`.
fn tile_owned_by(
    hex_map: &HexMap,
    coord: HexCoord,
    provinces: &[Province],
    nation_id: NationId,
) -> bool {
    let Some(tile) = hex_map.get_tile(coord) else {
        return false;
    };
    let Some(pid) = tile.province_id else {
        return false;
    };
    provinces
        .iter()
        .any(|p| p.id == pid && p.owner == nation_id)
}

/// Build a railroad on a tile. Caller must own the tile's province AND have
/// researched any tech the terrain requires. Returns cost or error.
pub fn build_railroad(
    hex_map: &mut HexMap,
    coord: HexCoord,
    nation_id: NationId,
    researched_techs: &[crate::events::TechId],
    provinces: &[Province],
    game_data: &GameData,
    cfg: &GameConfig,
) -> Result<Money, DomainError> {
    if !tile_owned_by(hex_map, coord, provinces, nation_id) {
        return Err(DomainError::illegal("Cannot build railroad on tile not owned by this nation"));
    }
    let tile = hex_map.get_tile(coord).ok_or(DomainError::TileNotFound(coord))?;
    if tile.infrastructure.has_railroad {
        return Err(DomainError::illegal("Railroad already exists"));
    }
    let terrain = tile.terrain();
    let cost = railroad_cost(terrain, cfg).ok_or(DomainError::illegal("Cannot build railroad on sea"))?;
    if !rail_terrain_enabled(terrain, researched_techs, game_data, cfg) {
        let tech = railroad_required_tech(terrain, cfg).unwrap_or("?");
        return Err(DomainError::illegal(format!("Railroad on {:?} requires tech: {}", terrain, tech)));
    }

    let tile = hex_map.get_tile_mut(coord).ok_or(DomainError::TileNotFound(coord))?;
    tile.infrastructure.has_railroad = true;
    Ok(cost)
}

/// Build a depot on a tile. Caller must own the tile's province. The tile must
/// have a railroad OR be the nation's capital tile (which has an implicit depot).
pub fn build_depot(
    hex_map: &mut HexMap,
    coord: HexCoord,
    nation_id: NationId,
    provinces: &[Province],
    cfg: &GameConfig,
) -> Result<Money, DomainError> {
    if !tile_owned_by(hex_map, coord, provinces, nation_id) {
        return Err(DomainError::illegal("Cannot build depot on tile not owned by this nation"));
    }
    let tile = hex_map.get_tile(coord).ok_or(DomainError::TileNotFound(coord))?;
    if !tile.terrain().is_land() {
        return Err(DomainError::illegal("Cannot build depot on sea"));
    }
    if tile.infrastructure.has_depot {
        return Err(DomainError::illegal("Depot already exists"));
    }
    if !tile.infrastructure.has_railroad {
        return Err(DomainError::illegal("Depot requires a railroad on the tile"));
    }
    let tile = hex_map.get_tile_mut(coord).ok_or(DomainError::TileNotFound(coord))?;
    tile.infrastructure.has_depot = true;
    Ok(Money::dollars(cfg.depot_cost))
}

/// Unchecked depot placement used during scenario setup (pre-builds the capital's
/// depot before the prerequisite rules are in force).
pub fn place_depot_unchecked(hex_map: &mut HexMap, coord: HexCoord) -> Result<(), DomainError> {
    let tile = hex_map.get_tile_mut(coord).ok_or(DomainError::TileNotFound(coord))?;
    if !tile.terrain().is_land() {
        return Err(DomainError::illegal("Cannot place depot on sea"));
    }
    tile.infrastructure.has_depot = true;
    Ok(())
}

/// True if `coord` has at least one neighbor that is a sea tile by terrain.
/// Used by `build_port` to preserve the historical port-placement rule
/// (lake-coast tiles still allow port construction).
fn coord_is_sea_adjacent(hex_map: &HexMap, coord: HexCoord) -> bool {
    coord
        .neighbors()
        .iter()
        .any(|n| hex_map.get_tile(*n).is_some_and(|t| !t.terrain().is_land()))
}

/// True if `coord` has at least one neighbor that lies in a non-lake ocean
/// `SeaZone`. This is the predicate used for sea-trade connectivity (cards
/// #408, #419, #421): only ocean-facing ports/capitals act as sea hubs.
///
/// When `sea_zones` is empty (e.g. unit tests that don't construct zones),
/// this falls back to terrain-based adjacency so existing test setups keep
/// working.
pub fn coord_has_ocean_neighbor(
    hex_map: &HexMap,
    sea_zones: &[crate::map::sea_zones::SeaZone],
    coord: HexCoord,
) -> bool {
    if sea_zones.is_empty() {
        return coord_is_sea_adjacent(hex_map, coord);
    }
    coord.neighbors().iter().any(|n| {
        sea_zones.iter().any(|z| !z.is_lake && z.hexes.contains(n))
    })
}

/// True if a tile acts as a port for connectivity purposes — a literal built
/// port adjacent to ocean, or a country-capital tile adjacent to ocean.
///
/// Card #419: a country capital adjacent to the ocean is treated as having an
/// implicit port. F-001 review fix: only ocean-adjacent ports/capitals count
/// for sea-trade connectivity — lake-side ports do not, even when constructed.
///
/// `sea_zones` empty → terrain-only fallback (test mode).
pub fn has_effective_port(
    hex_map: &HexMap,
    sea_zones: &[crate::map::sea_zones::SeaZone],
    coord: HexCoord,
) -> bool {
    has_effective_port_filtered(hex_map, sea_zones, coord, &HashSet::new())
}

/// Like `has_effective_port` but treats any port tile in `blockaded_ports` as
/// disconnected. Card #408: a port adjacent to a sea zone under undisputed
/// enemy command loses its sea-trade link until a friendly warship returns to
/// the zone.
pub fn has_effective_port_filtered(
    hex_map: &HexMap,
    sea_zones: &[crate::map::sea_zones::SeaZone],
    coord: HexCoord,
    blockaded_ports: &HashSet<HexCoord>,
) -> bool {
    if blockaded_ports.contains(&coord) {
        return false;
    }
    let Some(tile) = hex_map.get_tile(coord) else {
        return false;
    };
    let ocean_adjacent = coord_has_ocean_neighbor(hex_map, sea_zones, coord);
    if tile.infrastructure.has_port && ocean_adjacent {
        return true;
    }
    if tile.is_country_capital && ocean_adjacent {
        return true;
    }
    false
}

/// Build a port on a coastal tile. Caller must own the tile's province.
pub fn build_port(
    hex_map: &mut HexMap,
    coord: HexCoord,
    nation_id: NationId,
    provinces: &[Province],
    cfg: &GameConfig,
) -> Result<Money, DomainError> {
    if !tile_owned_by(hex_map, coord, provinces, nation_id) {
        return Err(DomainError::illegal("Cannot build port on tile not owned by this nation"));
    }
    let tile = hex_map.get_tile(coord).ok_or(DomainError::TileNotFound(coord))?;
    if !tile.terrain().is_land() {
        return Err(DomainError::illegal("Cannot build port on sea"));
    }
    if tile.infrastructure.has_port {
        return Err(DomainError::illegal("Port already exists"));
    }
    if !coord_is_sea_adjacent(hex_map, coord) {
        return Err(DomainError::illegal("Port must be on a coastal tile"));
    }
    let tile = hex_map.get_tile_mut(coord).ok_or(DomainError::TileNotFound(coord))?;
    tile.infrastructure.has_port = true;
    Ok(Money::dollars(cfg.port_cost))
}

/// Cost to build or upgrade a fort.
pub fn fort_cost(level: u8, cfg: &GameConfig) -> Result<Money, DomainError> {
    match level {
        1 => Ok(Money::dollars(cfg.fort_cost_level_1)),
        2 => Ok(Money::dollars(cfg.fort_cost_level_2)),
        3 => Ok(Money::dollars(cfg.fort_cost_level_3)),
        _ => Err(DomainError::illegal("Fort level must be 1-3")),
    }
}

/// Build or upgrade a fort on a tile. Returns (new_level, cost).
pub fn build_fort(
    hex_map: &mut HexMap,
    coord: HexCoord,
    cfg: &GameConfig,
) -> Result<(u8, Money), DomainError> {
    let tile = hex_map.get_tile(coord).ok_or(DomainError::TileNotFound(coord))?;
    if !tile.terrain().is_land() {
        return Err(DomainError::illegal("Cannot build fort on sea"));
    }
    let current_level = tile.infrastructure.fort_level;
    if current_level >= 3 {
        return Err(DomainError::illegal("Fort already at maximum level (3)"));
    }
    let new_level = current_level + 1;
    let cost = fort_cost(new_level, cfg)?;

    let tile = hex_map.get_tile_mut(coord).ok_or(DomainError::TileNotFound(coord))?;
    tile.infrastructure.has_fort = true;
    tile.infrastructure.fort_level = new_level;
    Ok((new_level, cost))
}

/// Set of hexes a nation can harvest this turn under the unified collector
/// model (cards #130 + #131).
///
/// A tile is a *collector* if either
///   * it has a depot AND its province is in `connected` (reachable from a
///     country capital by rail/port chain), or
///   * `tile.is_country_capital` is set (country capitals are their own hubs,
///     rail-independent — this keeps conquered foreign capitals productive
///     even when the rail link is lost).
///
/// Each collector emits its own hex plus every owned neighboring hex. The
/// `HashSet` return type deduplicates overlapping collector radii, which makes
/// the "each tile yields at most once" guarantee load-bearing for card #131.
pub fn collectable_hexes(
    hex_map: &HexMap,
    owned_provinces: &[&Province],
    connected: &HashSet<ProvinceId>,
) -> HashSet<HexCoord> {
    let mut out: HashSet<HexCoord> = HashSet::new();
    // Owned-tile lookup so a collector's radius never leaks onto enemy hexes.
    let owned_tiles: HashSet<HexCoord> = owned_provinces
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .collect();

    for province in owned_provinces {
        let province_connected = connected.contains(&province.id);
        for &tile_coord in &province.tiles {
            let Some(tile) = hex_map.get_tile(tile_coord) else {
                continue;
            };
            let connected_depot = tile.infrastructure.has_depot && province_connected;
            let is_collector = connected_depot || tile.is_country_capital;
            if !is_collector {
                continue;
            }
            out.insert(tile_coord);
            for neighbor in tile_coord.neighbors() {
                if owned_tiles.contains(&neighbor) {
                    out.insert(neighbor);
                }
            }
        }
    }

    out
}

/// Check if a province is connected to the national capital via railroad
/// network or port chain. Single-seed convenience wrapper; use
/// `is_province_connected_multi` when captured country-capital tiles should
/// also act as seeds.
pub fn is_province_connected(
    hex_map: &HexMap,
    capital_tile: HexCoord,
    target_province_id: ProvinceId,
    provinces: &[Province],
) -> bool {
    is_province_connected_multi(hex_map, &[capital_tile], target_province_id, provinces)
}

/// Like `is_province_connected` but seeds BFS from every tile in
/// `capital_tiles`. Use the nation's own capital PLUS every owned
/// country-capital tile so captured foreign capitals behave as independent hubs
/// without requiring a rail chain back to the home capital.
pub fn is_province_connected_multi(
    hex_map: &HexMap,
    capital_tiles: &[HexCoord],
    target_province_id: ProvinceId,
    provinces: &[Province],
) -> bool {
    is_province_connected_multi_filtered(
        hex_map,
        &[],
        capital_tiles,
        target_province_id,
        provinces,
        &HashSet::new(),
    )
}

/// Like `is_province_connected_multi` but ignores any port whose tile is in
/// `blockaded_ports` and uses zone-aware port detection (only ocean — not
/// lake — adjacency counts). Card #408 + F-001 review fix.
pub fn is_province_connected_multi_filtered(
    hex_map: &HexMap,
    sea_zones: &[crate::map::sea_zones::SeaZone],
    capital_tiles: &[HexCoord],
    target_province_id: ProvinceId,
    provinces: &[Province],
    blockaded_ports: &HashSet<HexCoord>,
) -> bool {
    // Shortcut: if the target province contains any of the seed tiles (the
    // nation's capital or a captured country capital), it's trivially connected.
    if let Some(prov) = provinces.iter().find(|p| p.id == target_province_id) {
        for &seed in capital_tiles {
            if prov.tiles.contains(&seed) {
                return true;
            }
        }
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let seed_set: HashSet<HexCoord> = capital_tiles.iter().copied().collect();
    for &c in capital_tiles {
        queue.push_back(c);
        visited.insert(c);
    }

    let any_seed_has_port = capital_tiles
        .iter()
        .any(|c| has_effective_port_filtered(hex_map, sea_zones, *c, blockaded_ports));

    while let Some(current) = queue.pop_front() {
        if let Some(tile) = hex_map.get_tile(current) {
            if tile.province_id == Some(target_province_id) && tile.infrastructure.has_depot {
                return true;
            }
            if tile.infrastructure.has_railroad || seed_set.contains(&current) {
                for neighbor in current.neighbors() {
                    if !visited.contains(&neighbor)
                        && let Some(n_tile) = hex_map.get_tile(neighbor)
                        && (n_tile.infrastructure.has_railroad || n_tile.infrastructure.has_depot)
                    {
                        visited.insert(neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    // Port-to-port: if any seed tile has a port (real or capital-implicit) and
    // the target province has a port (real or capital-implicit), they are
    // connected by sea (card #419). Blockaded ports are skipped (card #408).
    if any_seed_has_port {
        let target_prov = provinces.iter().find(|p| p.id == target_province_id);
        if let Some(prov) = target_prov {
            for tile_coord in &prov.tiles {
                if has_effective_port_filtered(hex_map, sea_zones, *tile_coord, blockaded_ports) {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::map::tile::Tile;

    fn cfg() -> GameConfig {
        GameConfig::default()
    }

    /// Thin wrapper so existing tests don't need to thread tech state — uses
    /// an empty researched-techs list and the default `GameData`.
    fn test_build_railroad(
        map: &mut HexMap,
        coord: HexCoord,
        nation_id: NationId,
        provinces: &[Province],
    ) -> Result<Money, DomainError> {
        build_railroad(
            map,
            coord,
            nation_id,
            &[],
            provinces,
            &GameData::default(),
            &cfg(),
        )
    }

    /// Test helper: place a land tile owned by nation 1 (province 1) at `coord`.
    fn owned_land(map: &mut HexMap, coord: HexCoord, terrain: TerrainType) -> Vec<Province> {
        map.set_tile(coord, Tile::with_province(terrain, ProvinceId(1)));
        vec![Province::new(
            ProvinceId(1),
            "Own".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        )]
    }

    // ── railroad_cost ─────────────────────────────────────────

    #[test]
    fn railroad_cost_standard_land() {
        let c = cfg();
        for terrain in [TerrainType::Grassland, TerrainType::Forest] {
            assert_eq!(
                railroad_cost(terrain, &c),
                Some(Money::dollars(100)),
                "Expected $100 for {:?}",
                terrain
            );
        }
    }

    #[test]
    fn railroad_cost_desert_tundra() {
        let c = cfg();
        assert_eq!(
            railroad_cost(TerrainType::Desert, &c),
            Some(Money::dollars(150))
        );
        assert_eq!(
            railroad_cost(TerrainType::Tundra, &c),
            Some(Money::dollars(150))
        );
    }

    #[test]
    fn railroad_cost_swamp() {
        assert_eq!(
            railroad_cost(TerrainType::Swamp, &cfg()),
            Some(Money::dollars(300))
        );
    }

    #[test]
    fn railroad_cost_hills() {
        assert_eq!(
            railroad_cost(TerrainType::Hills, &cfg()),
            Some(Money::dollars(200))
        );
    }

    #[test]
    fn railroad_cost_mountain() {
        assert_eq!(
            railroad_cost(TerrainType::Mountain, &cfg()),
            Some(Money::dollars(500))
        );
    }

    #[test]
    fn railroad_cost_sea_returns_none() {
        assert_eq!(railroad_cost(TerrainType::Sea, &cfg()), None);
    }

    // ── build_railroad ────────────────────────────────────────

    #[test]
    fn build_railroad_on_valid_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Grassland);
        let result = test_build_railroad(&mut map, coord, NationId(1), &provinces);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Money::dollars(100));
        assert!(map.get_tile(coord).unwrap().infrastructure.has_railroad);
    }

    #[test]
    fn build_railroad_requires_ownership() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(
            coord,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        let provinces = vec![Province::new(
            ProvinceId(1),
            "Foreign".to_string(),
            NationId(7),
            coord,
            vec![coord],
            4,
        )];
        let result = test_build_railroad(&mut map, coord, NationId(1), &provinces);
        assert!(result.is_err());
    }

    #[test]
    fn build_railroad_already_exists() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Grassland);
        test_build_railroad(&mut map, coord, NationId(1), &provinces).unwrap();
        let result = test_build_railroad(&mut map, coord, NationId(1), &provinces);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "illegal move: Railroad already exists");
    }

    #[test]
    fn build_railroad_on_sea_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Sea);
        let result = test_build_railroad(&mut map, coord, NationId(1), &provinces);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "illegal move: Cannot build railroad on sea");
    }

    #[test]
    fn build_railroad_tile_not_found() {
        let mut map = HexMap::new(10, 10);
        let result = test_build_railroad(&mut map, HexCoord::new(5, 5), NationId(1), &[]);
        assert!(result.is_err());
    }

    // ── build_depot ───────────────────────────────────────────

    #[test]
    fn build_depot_on_railroad_hex() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Grassland);
        // Build a railroad first so the depot prerequisite is met
        test_build_railroad(&mut map, coord, NationId(1), &provinces).unwrap();
        let result = build_depot(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Money::dollars(2000));
        assert!(map.get_tile(coord).unwrap().infrastructure.has_depot);
    }

    #[test]
    fn build_depot_rejects_capital_tile_without_railroad() {
        // Depots require an actual railroad; the `is_capital` flag is set on
        // every province's centroid (not just the nation's capital) so bypassing
        // the rail prerequisite based on that flag is wrong. Starting-state
        // capital depots are placed via `place_depot_unchecked` at setup,
        // not through `build_depot`.
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let mut tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        tile.is_capital = true;
        map.set_tile(coord, tile);
        let provinces = vec![Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        )];
        let result = build_depot(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
    }

    #[test]
    fn build_depot_rejected_without_railroad() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Grassland);
        let result = build_depot(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
    }

    #[test]
    fn build_depot_already_exists() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Grassland);
        test_build_railroad(&mut map, coord, NationId(1), &provinces).unwrap();
        build_depot(&mut map, coord, NationId(1), &provinces, &cfg()).unwrap();
        let result = build_depot(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "illegal move: Depot already exists");
    }

    #[test]
    fn build_depot_on_sea_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Sea);
        let result = build_depot(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
    }

    // ── build_port ────────────────────────────────────────────

    #[test]
    fn build_port_on_coastal_tile() {
        let mut map = HexMap::new(10, 10);
        let land = HexCoord::new(1, 0);
        let sea = HexCoord::new(2, 0);
        map.set_tile(
            land,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(sea, Tile::new(TerrainType::Sea));
        let provinces = vec![Province::new(
            ProvinceId(1),
            "Own".to_string(),
            NationId(1),
            land,
            vec![land],
            4,
        )];
        let result = build_port(&mut map, land, NationId(1), &provinces, &cfg());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Money::dollars(3000));
        assert!(map.get_tile(land).unwrap().infrastructure.has_port);
    }

    #[test]
    fn build_port_on_non_coastal_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(5, 5);
        let provinces = owned_land(&mut map, coord, TerrainType::Grassland);
        let result = build_port(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "illegal move: Port must be on a coastal tile"
        );
    }

    #[test]
    fn build_port_on_sea_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Sea);
        let result = build_port(&mut map, coord, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
    }

    #[test]
    fn build_port_already_exists() {
        let mut map = HexMap::new(10, 10);
        let land = HexCoord::new(1, 0);
        let sea = HexCoord::new(2, 0);
        map.set_tile(
            land,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(sea, Tile::new(TerrainType::Sea));
        let provinces = vec![Province::new(
            ProvinceId(1),
            "Own".to_string(),
            NationId(1),
            land,
            vec![land],
            4,
        )];
        build_port(&mut map, land, NationId(1), &provinces, &cfg()).unwrap();
        let result = build_port(&mut map, land, NationId(1), &provinces, &cfg());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "illegal move: Port already exists");
    }

    // ── is_province_connected ─────────────────────────────────

    #[test]
    fn connected_via_railroad_chain() {
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let mid_coord = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(2, 0);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        // Capital tile (province 1)
        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        map.set_tile(capital_coord, capital_tile);

        // Middle tile with railroad (province 1)
        let mut mid_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        mid_tile.infrastructure.has_railroad = true;
        map.set_tile(mid_coord, mid_tile);

        // Target tile with depot (province 2)
        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_depot = true;
        map.set_tile(target_coord, target_tile);

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord, mid_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        assert!(is_province_connected(
            &map,
            capital_coord,
            target_pid,
            &provinces
        ));
    }

    #[test]
    fn not_connected_no_railroad() {
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let target_coord = HexCoord::new(5, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        map.set_tile(capital_coord, capital_tile);

        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_depot = true;
        map.set_tile(target_coord, target_tile);

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        assert!(!is_province_connected(
            &map,
            capital_coord,
            target_pid,
            &provinces
        ));
    }

    #[test]
    fn connected_via_ports() {
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let sea_near_capital = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(5, 5);
        let sea_near_target = HexCoord::new(6, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        // Capital tile with port
        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);

        // Sea neighbor for capital (makes it coastal)
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Target tile with port
        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_port = true;
        map.set_tile(target_coord, target_tile);

        // Sea neighbor for target
        map.set_tile(sea_near_target, Tile::new(TerrainType::Sea));

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        assert!(is_province_connected(
            &map,
            capital_coord,
            target_pid,
            &provinces
        ));
    }

    #[test]
    fn lake_adjacent_port_does_not_grant_sea_connectivity() {
        // F-001 review fix: a port adjacent only to a lake (not ocean) must
        // not act as a sea hub, even though it's a built port. Two such
        // ports facing the same lake are NOT connected by sea.
        use crate::map::sea_zones::{SeaZone, SeaZoneId};
        use std::collections::BTreeSet;

        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let lake_near_capital = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(5, 5);
        let lake_near_target = HexCoord::new(6, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.is_country_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);
        map.set_tile(lake_near_capital, Tile::new(TerrainType::Sea));

        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_port = true;
        map.set_tile(target_coord, target_tile);
        map.set_tile(lake_near_target, Tile::new(TerrainType::Sea));

        // Both sea hexes belong to lake zones (is_lake = true), not ocean.
        let lake_a = SeaZone {
            id: SeaZoneId(0),
            name: "Lake A".to_string(),
            hexes: BTreeSet::from([lake_near_capital]),
            is_lake: true,
            adjacent_zone_ids: Vec::new(),
            coastal_provinces: Vec::new(),
        };
        let lake_b = SeaZone {
            id: SeaZoneId(1),
            name: "Lake B".to_string(),
            hexes: BTreeSet::from([lake_near_target]),
            is_lake: true,
            adjacent_zone_ids: Vec::new(),
            coastal_provinces: Vec::new(),
        };
        let zones = vec![lake_a, lake_b];

        let provinces = vec![
            Province::new(capital_pid, "Capital".into(), NationId(1), capital_coord, vec![capital_coord], 4),
            Province::new(target_pid, "Target".into(), NationId(1), target_coord, vec![target_coord], 3),
        ];

        assert!(
            !is_province_connected_multi_filtered(
                &map,
                &zones,
                &[capital_coord],
                target_pid,
                &provinces,
                &HashSet::new(),
            ),
            "two lake-side ports must NOT establish sea-trade connectivity"
        );
    }

    #[test]
    fn coastal_capital_acts_as_port_without_built_port() {
        // Card #419: a country-capital tile adjacent to the sea is treated as
        // having an implicit port for connectivity, so a remote province with
        // a built port is reachable even when no port has been built on the
        // capital itself.
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let sea_near_capital = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(5, 5);
        let sea_near_target = HexCoord::new(6, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        // Capital tile WITHOUT a built port — only the country-capital flag and
        // sea adjacency.
        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.is_country_capital = true;
        map.set_tile(capital_coord, capital_tile);
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Target province has a real, built port.
        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_port = true;
        map.set_tile(target_coord, target_tile);
        map.set_tile(sea_near_target, Tile::new(TerrainType::Sea));

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        assert!(
            is_province_connected(&map, capital_coord, target_pid, &provinces),
            "coastal country capital must act as a port without a built port"
        );
    }

    #[test]
    fn inland_capital_does_not_act_as_port() {
        // Inverse of the above: a country-capital tile NOT adjacent to sea
        // gets no implicit port, so port-only routes don't reach it.
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let target_coord = HexCoord::new(5, 5);
        let sea_near_target = HexCoord::new(6, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_country_capital = true;
        // No sea-adjacent neighbor — capital is inland.
        map.set_tile(capital_coord, capital_tile);

        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_port = true;
        map.set_tile(target_coord, target_tile);
        map.set_tile(sea_near_target, Tile::new(TerrainType::Sea));

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        assert!(
            !is_province_connected(&map, capital_coord, target_pid, &provinces),
            "inland country capital must NOT act as a port"
        );
    }

    #[test]
    fn not_connected_capital_has_port_target_does_not() {
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let sea_near_capital = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(5, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Target tile without port
        let target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        map.set_tile(target_coord, target_tile);

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        assert!(!is_province_connected(
            &map,
            capital_coord,
            target_pid,
            &provinces
        ));
    }

    // ── fort ─────────────────────────────────────────────────

    #[test]
    fn fort_cost_levels() {
        let c = cfg();
        assert_eq!(fort_cost(1, &c), Ok(Money::dollars(5000)));
        assert_eq!(fort_cost(2, &c), Ok(Money::dollars(7500)));
        assert_eq!(fort_cost(3, &c), Ok(Money::dollars(10000)));
        assert!(fort_cost(4, &c).is_err());
    }

    #[test]
    fn build_fort_level_1() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Grassland));

        let result = build_fort(&mut map, coord, &cfg());
        assert!(result.is_ok());
        let (level, cost) = result.unwrap();
        assert_eq!(level, 1);
        assert_eq!(cost, Money::dollars(5000));
        assert!(map.get_tile(coord).unwrap().infrastructure.has_fort);
        assert_eq!(map.get_tile(coord).unwrap().infrastructure.fort_level, 1);
    }

    #[test]
    fn build_fort_upgrades_to_level_2() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Grassland));

        build_fort(&mut map, coord, &cfg()).unwrap();
        let (level, cost) = build_fort(&mut map, coord, &cfg()).unwrap();
        assert_eq!(level, 2);
        assert_eq!(cost, Money::dollars(7500));
    }

    #[test]
    fn build_fort_max_level_3() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Grassland));

        build_fort(&mut map, coord, &cfg()).unwrap(); // L1
        build_fort(&mut map, coord, &cfg()).unwrap(); // L2
        build_fort(&mut map, coord, &cfg()).unwrap(); // L3
        let result = build_fort(&mut map, coord, &cfg());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "illegal move: Fort already at maximum level (3)"
        );
    }

    #[test]
    fn build_fort_on_sea_fails() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Sea));

        let result = build_fort(&mut map, coord, &cfg());
        assert!(result.is_err());
    }

    // ── collectable_hexes ─────────────────────────────────────

    #[test]
    fn collectable_hexes_capital_tile_yields_one_hex_radius_only() {
        // Card #130: the capital tile acts as a depot (own hex + 1-hex radius).
        // Far tiles in the same province do NOT yield just because they share
        // the capital's province — they need their own depot.
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0);
        let neighbor = cap.neighbors()[0];
        let far = HexCoord::new(5, 5);
        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        cap_tile.is_country_capital = true;
        cap_tile.infrastructure.has_depot = true;
        map.set_tile(cap, cap_tile);
        map.set_tile(
            neighbor,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(
            far,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        let capital = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap, neighbor, far],
            4,
        );
        let provinces_ref: Vec<&Province> = vec![&capital];
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1)]);
        let set = collectable_hexes(&map, &provinces_ref, &connected);
        assert!(set.contains(&cap), "capital tile is a collector");
        assert!(set.contains(&neighbor), "neighbor is in 1-hex radius");
        assert!(
            !set.contains(&far),
            "far tile in capital province does NOT yield under the unified model"
        );
    }

    #[test]
    fn collectable_hexes_depot_covers_one_hex_radius() {
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0);
        let depot_hex = HexCoord::new(4, 0);
        let neighbor = depot_hex.neighbors()[0];
        let far = HexCoord::new(8, 8);
        map.set_tile(
            cap,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        let mut dh = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        dh.infrastructure.has_depot = true;
        map.set_tile(depot_hex, dh);
        map.set_tile(
            neighbor,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        map.set_tile(
            far,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        let capital = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap],
            4,
        );
        let target = Province::new(
            ProvinceId(2),
            "T".to_string(),
            NationId(1),
            depot_hex,
            vec![depot_hex, neighbor, far],
            3,
        );
        let provinces_ref: Vec<&Province> = vec![&capital, &target];
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1), ProvinceId(2)]);
        let set = collectable_hexes(&map, &provinces_ref, &connected);
        assert!(set.contains(&depot_hex));
        assert!(set.contains(&neighbor));
        assert!(!set.contains(&far), "far tile is outside depot radius");
    }

    #[test]
    fn collectable_hexes_country_capital_yields_one_hex_radius() {
        // Cards #130 + #131: a captured country capital acts as a depot
        // (own hex + 1-hex owned-neighbor radius), rail-independent. The
        // previous "whole province yields" rule is gone — far tiles need
        // their own depot.
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0);
        let captured_cap = HexCoord::new(4, 0);
        let neighbor_of_captured = captured_cap.neighbors()[0];
        let far = HexCoord::new(8, 8);

        let mut own_cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        own_cap_tile.is_country_capital = true;
        own_cap_tile.infrastructure.has_depot = true;
        map.set_tile(cap, own_cap_tile);

        let mut cap_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        cap_tile.is_country_capital = true;
        map.set_tile(captured_cap, cap_tile);
        map.set_tile(
            neighbor_of_captured,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        map.set_tile(
            far,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let own_cap = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap],
            4,
        );
        let captured = Province::new(
            ProvinceId(2),
            "Captured".to_string(),
            NationId(1),
            captured_cap,
            vec![captured_cap, neighbor_of_captured, far],
            3,
        );
        let provinces_ref: Vec<&Province> = vec![&own_cap, &captured];
        // Captured province NOT in the connected set — country capital is
        // its own hub, rail-independent.
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1)]);
        let set = collectable_hexes(&map, &provinces_ref, &connected);

        assert!(
            set.contains(&captured_cap),
            "captured country-capital hex yields as an independent collector"
        );
        assert!(
            set.contains(&neighbor_of_captured),
            "neighbor within 1-hex radius of captured capital yields"
        );
        assert!(
            !set.contains(&far),
            "far tile in captured-capital province does NOT yield under the unified model"
        );
    }

    #[test]
    fn collectable_hexes_disconnected_province_yields_nothing() {
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0);
        let depot_hex = HexCoord::new(4, 0);
        map.set_tile(
            cap,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        let mut dh = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        dh.infrastructure.has_depot = true;
        map.set_tile(depot_hex, dh);
        let capital = Province::new(
            ProvinceId(1),
            "Cap".to_string(),
            NationId(1),
            cap,
            vec![cap],
            4,
        );
        let target = Province::new(
            ProvinceId(2),
            "T".to_string(),
            NationId(1),
            depot_hex,
            vec![depot_hex],
            3,
        );
        let provinces_ref: Vec<&Province> = vec![&capital, &target];
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1)]); // not 2
        let set = collectable_hexes(&map, &provinces_ref, &connected);
        assert!(!set.contains(&depot_hex));
    }

    #[test]
    fn collectable_hexes_overlapping_depots_dedup() {
        // Card #131: two depot radii overlap on one owned hex; the set
        // contains that hex exactly once (HashSet dedup is load-bearing
        // once collectors can overlap under the unified model).
        let mut map = HexMap::new(10, 10);
        let d1 = HexCoord::new(2, 0);
        let d2 = HexCoord::new(4, 0);
        // The two depots are 2 hexes apart, so their 1-hex radii overlap on
        // the tile between them.
        let shared_candidates: Vec<HexCoord> = d1
            .neighbors()
            .iter()
            .copied()
            .filter(|n| d2.neighbors().contains(n))
            .collect();
        assert!(
            !shared_candidates.is_empty(),
            "test precondition: depot radii must overlap"
        );
        let shared = shared_candidates[0];

        let mut t1 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        t1.infrastructure.has_depot = true;
        map.set_tile(d1, t1);
        let mut t2 = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        t2.infrastructure.has_depot = true;
        map.set_tile(d2, t2);
        map.set_tile(
            shared,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );

        let prov = Province::new(
            ProvinceId(1),
            "Only".to_string(),
            NationId(1),
            d1,
            vec![d1, d2, shared],
            4,
        );
        let provinces_ref: Vec<&Province> = vec![&prov];
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1)]);
        let set = collectable_hexes(&map, &provinces_ref, &connected);

        let shared_count = set.iter().filter(|h| **h == shared).count();
        assert_eq!(
            shared_count, 1,
            "overlap tile must appear exactly once (HashSet dedup)"
        );
    }

    #[test]
    fn collectable_hexes_province_capital_without_country_capital_yields_nothing() {
        // Card #130 regression: a province whose centroid has is_capital=true
        // (province capital) but NOT is_country_capital must not act as a
        // collector. Only an actual depot (or country-capital tile) makes a
        // province contribute yields.
        let mut map = HexMap::new(10, 10);
        let home_cap = HexCoord::new(0, 0);
        let prov_centroid = HexCoord::new(4, 0); // is_capital=true, no country capital
        let far = HexCoord::new(6, 0); // outside any collector radius

        // Home country capital (with implicit depot, our rail hub).
        let mut home_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        home_tile.is_country_capital = true;
        home_tile.infrastructure.has_depot = true;
        map.set_tile(home_cap, home_tile);

        // Province centroid: is_capital flag set, but NOT country capital,
        // and NO depot. Under the unified model this tile must yield nothing.
        let mut centroid_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        centroid_tile.is_capital = true;
        map.set_tile(prov_centroid, centroid_tile);
        map.set_tile(
            far,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let home = Province::new(
            ProvinceId(1),
            "Home".to_string(),
            NationId(1),
            home_cap,
            vec![home_cap],
            4,
        );
        let province = Province::new(
            ProvinceId(2),
            "P".to_string(),
            NationId(1),
            prov_centroid,
            vec![prov_centroid, far],
            3,
        );
        let provinces_ref: Vec<&Province> = vec![&home, &province];
        // Even if province 2 is "connected", no depot → no collector.
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1), ProvinceId(2)]);
        let set = collectable_hexes(&map, &provinces_ref, &connected);

        assert!(
            !set.contains(&prov_centroid),
            "province capital without a depot must not collect (card #130)"
        );
        assert!(
            !set.contains(&far),
            "far tile without any collector nearby does not yield"
        );
    }

    #[test]
    fn collectable_hexes_captured_country_capital_without_depot_still_yields_radius() {
        // Card #130 "even of conquered countries": the is_country_capital
        // flag persists through conquest and acts as an independent collector
        // seed EVEN if the actual depot has been removed.
        let mut map = HexMap::new(10, 10);
        let home_cap = HexCoord::new(0, 0);
        let captured = HexCoord::new(4, 0);
        let captured_neighbor = captured.neighbors()[0];

        let mut home_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(1));
        home_tile.is_country_capital = true;
        home_tile.infrastructure.has_depot = true;
        map.set_tile(home_cap, home_tile);

        // Captured country-capital tile: flag set but has_depot = false.
        let mut captured_tile = Tile::with_province(TerrainType::Grassland, ProvinceId(2));
        captured_tile.is_country_capital = true;
        // No depot.
        map.set_tile(captured, captured_tile);
        map.set_tile(
            captured_neighbor,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        let home = Province::new(
            ProvinceId(1),
            "Home".to_string(),
            NationId(1),
            home_cap,
            vec![home_cap],
            4,
        );
        let captured_prov = Province::new(
            ProvinceId(2),
            "Captured".to_string(),
            NationId(1),
            captured,
            vec![captured, captured_neighbor],
            3,
        );
        let provinces_ref: Vec<&Province> = vec![&home, &captured_prov];
        // Captured province NOT in connected set — country-capital seeds are
        // rail-independent.
        let connected: HashSet<ProvinceId> = HashSet::from([ProvinceId(1)]);
        let set = collectable_hexes(&map, &provinces_ref, &connected);

        assert!(
            set.contains(&captured),
            "is_country_capital is an independent collector seed even with no depot"
        );
        assert!(
            set.contains(&captured_neighbor),
            "captured country-capital collects its 1-hex neighbor radius"
        );
    }

    // ── rail_terrain_enabled / tech gate ─────────────────────────

    #[test]
    fn rail_terrain_enabled_no_tech_required() {
        let data = crate::data::test_game_data();
        let c = cfg();
        let no_techs: &[crate::events::TechId] = &[];
        for terrain in [
            TerrainType::Grassland,
            TerrainType::Forest,
            TerrainType::Desert,
            TerrainType::Tundra,
            TerrainType::Hills,
        ] {
            assert!(
                rail_terrain_enabled(terrain, no_techs, &data, &c),
                "{:?} should be buildable without tech",
                terrain
            );
        }
    }

    #[test]
    fn rail_terrain_enabled_sea_always_false() {
        let data = crate::data::test_game_data();
        assert!(!rail_terrain_enabled(TerrainType::Sea, &[], &data, &cfg()));
    }

    #[test]
    fn rail_terrain_enabled_swamp_requires_tech() {
        let data = crate::data::test_game_data();
        let c = cfg();
        // Without the tech, swamp is not enabled.
        assert!(!rail_terrain_enabled(TerrainType::Swamp, &[], &data, &c));
        // With the tech researched, swamp is enabled.
        let swamp_tech = data
            .tech_tree
            .get_by_name("Iron Railroad Bridge")
            .map(|t| t.id)
            .expect("Iron Railroad Bridge tech present");
        assert!(rail_terrain_enabled(
            TerrainType::Swamp,
            &[swamp_tech],
            &data,
            &c
        ));
    }

    #[test]
    fn rail_terrain_enabled_mountain_requires_tech() {
        let data = crate::data::test_game_data();
        let c = cfg();
        assert!(!rail_terrain_enabled(TerrainType::Mountain, &[], &data, &c));
        let mountain_tech = data
            .tech_tree
            .get_by_name("Compound Steam Engine")
            .map(|t| t.id)
            .expect("Compound Steam Engine tech present");
        assert!(rail_terrain_enabled(
            TerrainType::Mountain,
            &[mountain_tech],
            &data,
            &c
        ));
    }

    #[test]
    fn build_railroad_rejects_swamp_without_tech() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        let provinces = owned_land(&mut map, coord, TerrainType::Swamp);
        let data = crate::data::test_game_data();
        let result = build_railroad(&mut map, coord, NationId(1), &[], &provinces, &data, &cfg());
        assert!(
            result.is_err(),
            "swamp railroad without tech must fail, got {:?}",
            result
        );
        assert!(result.unwrap_err().to_string().contains("Iron Railroad Bridge"));
    }

    // ── connectivity edge cases ─────────────────────────────

    #[test]
    fn not_connected_target_has_depot_but_no_railroad_path() {
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        // Place a gap tile with no railroad between capital and target
        let gap_coord = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(2, 0);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        map.set_tile(capital_coord, capital_tile);

        // Gap tile: no railroad, no depot
        let gap_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        map.set_tile(gap_coord, gap_tile);

        // Target tile has depot but is not reachable
        let mut target_tile = Tile::with_province(TerrainType::Grassland, target_pid);
        target_tile.infrastructure.has_depot = true;
        map.set_tile(target_coord, target_tile);

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord, gap_coord],
                4,
            ),
            Province::new(
                target_pid,
                "Target".to_string(),
                NationId(1),
                target_coord,
                vec![target_coord],
                3,
            ),
        ];

        // Gap tile has no railroad, so BFS from capital cannot reach target
        assert!(!is_province_connected(
            &map,
            capital_coord,
            target_pid,
            &provinces
        ));
    }

    // ── Province connectivity via both railroad and port ─────────

    #[test]
    fn province_connectivity_via_railroad_and_port() {
        let mut map = HexMap::new(20, 20);

        let capital_coord = HexCoord::new(0, 0);
        let railroad_coord = HexCoord::new(1, 0);
        let depot_coord = HexCoord::new(2, 0);
        let port_province_coord = HexCoord::new(8, 8);
        let sea_near_capital = HexCoord::new(0, 1);
        let sea_near_port = HexCoord::new(9, 8);

        let capital_pid = ProvinceId(1);
        let railroad_pid = ProvinceId(2);
        let port_pid = ProvinceId(3);

        // Capital tile with a port (connected to sea)
        let mut capital_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Railroad chain from capital to province 2 (railroad pathway)
        let mut rr_tile = Tile::with_province(TerrainType::Grassland, capital_pid);
        rr_tile.infrastructure.has_railroad = true;
        map.set_tile(railroad_coord, rr_tile);

        let mut depot_tile = Tile::with_province(TerrainType::Grassland, railroad_pid);
        depot_tile.infrastructure.has_depot = true;
        map.set_tile(depot_coord, depot_tile);

        // Port province (connected via sea to capital's port)
        let mut port_tile = Tile::with_province(TerrainType::Grassland, port_pid);
        port_tile.infrastructure.has_port = true;
        map.set_tile(port_province_coord, port_tile);
        map.set_tile(sea_near_port, Tile::new(TerrainType::Sea));

        let provinces = vec![
            Province::new(
                capital_pid,
                "Capital".to_string(),
                NationId(1),
                capital_coord,
                vec![capital_coord, railroad_coord],
                4,
            ),
            Province::new(
                railroad_pid,
                "Railroad Province".to_string(),
                NationId(1),
                depot_coord,
                vec![depot_coord],
                3,
            ),
            Province::new(
                port_pid,
                "Port Province".to_string(),
                NationId(1),
                port_province_coord,
                vec![port_province_coord],
                3,
            ),
        ];

        // Province 2 connected via railroad
        assert!(
            is_province_connected(&map, capital_coord, railroad_pid, &provinces),
            "Province should be connected via railroad chain"
        );

        // Province 3 connected via port
        assert!(
            is_province_connected(&map, capital_coord, port_pid, &provinces),
            "Province should be connected via port-to-port sea route"
        );
    }
}
