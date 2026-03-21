use crate::hex::HexCoord;
use crate::map::hex_map::HexMap;
use crate::map::province::Province;
use crate::types::*;
use std::collections::{HashSet, VecDeque};

/// Cost to build a railroad on a terrain type.
/// Returns `None` for terrain where railroads cannot be built (Sea).
pub fn railroad_cost(terrain: TerrainType) -> Option<Money> {
    match terrain {
        TerrainType::Farm
        | TerrainType::DryPlains
        | TerrainType::HardwoodForest
        | TerrainType::ScrubForest
        | TerrainType::OpenRange
        | TerrainType::HorseRanch
        | TerrainType::Plantation
        | TerrainType::Orchard
        | TerrainType::FertileHills => Some(Money::dollars(100)),
        TerrainType::Desert | TerrainType::Tundra => Some(Money::dollars(150)),
        TerrainType::Swamp => Some(Money::dollars(300)),
        TerrainType::BarrenHills => Some(Money::dollars(200)),
        TerrainType::Mountain => Some(Money::dollars(500)),
        TerrainType::Sea => None,
    }
}

/// Build a railroad on a tile. Returns cost or error.
pub fn build_railroad(hex_map: &mut HexMap, coord: HexCoord) -> Result<Money, String> {
    let tile = hex_map.get_tile(coord).ok_or("Tile not found")?;
    if tile.infrastructure.has_railroad {
        return Err("Railroad already exists".to_string());
    }
    let terrain = tile.terrain();
    let cost = railroad_cost(terrain).ok_or("Cannot build railroad on sea")?;

    let tile = hex_map.get_tile_mut(coord).ok_or("Tile not found")?;
    tile.infrastructure.has_railroad = true;
    Ok(cost)
}

/// Build a depot on a tile. Cost: $2,000.
pub fn build_depot(hex_map: &mut HexMap, coord: HexCoord) -> Result<Money, String> {
    let tile = hex_map.get_tile(coord).ok_or("Tile not found")?;
    if !tile.terrain().is_land() {
        return Err("Cannot build depot on sea".to_string());
    }
    if tile.infrastructure.has_depot {
        return Err("Depot already exists".to_string());
    }
    let tile = hex_map.get_tile_mut(coord).ok_or("Tile not found")?;
    tile.infrastructure.has_depot = true;
    Ok(Money::dollars(2000))
}

/// Build a port on a coastal tile. Cost: $3,000.
/// A coastal tile is a land tile that has at least one sea neighbor.
pub fn build_port(hex_map: &mut HexMap, coord: HexCoord) -> Result<Money, String> {
    let tile = hex_map.get_tile(coord).ok_or("Tile not found")?;
    if !tile.terrain().is_land() {
        return Err("Cannot build port on sea".to_string());
    }
    if tile.infrastructure.has_port {
        return Err("Port already exists".to_string());
    }
    let is_coastal = coord
        .neighbors()
        .iter()
        .any(|n| hex_map.get_tile(*n).is_some_and(|t| !t.terrain().is_land()));
    if !is_coastal {
        return Err("Port must be on a coastal tile".to_string());
    }
    let tile = hex_map.get_tile_mut(coord).ok_or("Tile not found")?;
    tile.infrastructure.has_port = true;
    Ok(Money::dollars(3000))
}

/// Cost to build or upgrade a fort.
pub fn fort_cost(level: u8) -> Result<Money, String> {
    match level {
        1 => Ok(Money::dollars(5000)),
        2 => Ok(Money::dollars(7500)),
        3 => Ok(Money::dollars(10000)),
        _ => Err("Fort level must be 1-3".to_string()),
    }
}

/// Build or upgrade a fort on a tile. Returns (new_level, cost).
pub fn build_fort(hex_map: &mut HexMap, coord: HexCoord) -> Result<(u8, Money), String> {
    let tile = hex_map.get_tile(coord).ok_or("Tile not found")?;
    if !tile.terrain().is_land() {
        return Err("Cannot build fort on sea".to_string());
    }
    let current_level = tile.infrastructure.fort_level;
    if current_level >= 3 {
        return Err("Fort already at maximum level (3)".to_string());
    }
    let new_level = current_level + 1;
    let cost = fort_cost(new_level)?;

    let tile = hex_map.get_tile_mut(coord).ok_or("Tile not found")?;
    tile.infrastructure.has_fort = true;
    tile.infrastructure.fort_level = new_level;
    Ok((new_level, cost))
}

/// Check if a province is connected to the national capital via railroad
/// network or port chain.
///
/// BFS from the capital tile following tiles that have railroads. If we reach
/// any tile belonging to `target_province_id` that has a depot, return true.
/// Also: if the capital province has a port AND the target province has a port,
/// they are connected via sea.
pub fn is_province_connected(
    hex_map: &HexMap,
    capital_tile: HexCoord,
    target_province_id: ProvinceId,
    provinces: &[Province],
) -> bool {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(capital_tile);
    visited.insert(capital_tile);

    let capital_has_port = hex_map
        .get_tile(capital_tile)
        .is_some_and(|t| t.infrastructure.has_port);

    while let Some(current) = queue.pop_front() {
        if let Some(tile) = hex_map.get_tile(current) {
            if tile.province_id == Some(target_province_id) && tile.infrastructure.has_depot {
                return true;
            }
            // Follow railroad connections (capital tile is always traversable)
            if tile.infrastructure.has_railroad || current == capital_tile {
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

    // Check port connection: if capital province has a port and target
    // province has a port, they are connected via sea.
    if capital_has_port {
        let target_prov = provinces.iter().find(|p| p.id == target_province_id);
        if let Some(prov) = target_prov {
            for tile_coord in &prov.tiles {
                if let Some(tile) = hex_map.get_tile(*tile_coord)
                    && tile.infrastructure.has_port
                {
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
    use crate::map::tile::Tile;

    // ── railroad_cost ─────────────────────────────────────────

    #[test]
    fn railroad_cost_standard_land() {
        let standard_terrains = [
            TerrainType::Farm,
            TerrainType::DryPlains,
            TerrainType::HardwoodForest,
            TerrainType::ScrubForest,
            TerrainType::OpenRange,
            TerrainType::HorseRanch,
            TerrainType::Plantation,
            TerrainType::Orchard,
            TerrainType::FertileHills,
        ];
        for terrain in standard_terrains {
            assert_eq!(
                railroad_cost(terrain),
                Some(Money::dollars(100)),
                "Expected $100 for {:?}",
                terrain
            );
        }
    }

    #[test]
    fn railroad_cost_desert_tundra() {
        assert_eq!(
            railroad_cost(TerrainType::Desert),
            Some(Money::dollars(150))
        );
        assert_eq!(
            railroad_cost(TerrainType::Tundra),
            Some(Money::dollars(150))
        );
    }

    #[test]
    fn railroad_cost_swamp() {
        assert_eq!(railroad_cost(TerrainType::Swamp), Some(Money::dollars(300)));
    }

    #[test]
    fn railroad_cost_barren_hills() {
        assert_eq!(
            railroad_cost(TerrainType::BarrenHills),
            Some(Money::dollars(200))
        );
    }

    #[test]
    fn railroad_cost_mountain() {
        assert_eq!(
            railroad_cost(TerrainType::Mountain),
            Some(Money::dollars(500))
        );
    }

    #[test]
    fn railroad_cost_sea_returns_none() {
        assert_eq!(railroad_cost(TerrainType::Sea), None);
    }

    // ── build_railroad ────────────────────────────────────────

    #[test]
    fn build_railroad_on_valid_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        let result = build_railroad(&mut map, coord);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Money::dollars(100));
        assert!(map.get_tile(coord).unwrap().infrastructure.has_railroad);
    }

    #[test]
    fn build_railroad_already_exists() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        build_railroad(&mut map, coord).unwrap();
        let result = build_railroad(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Railroad already exists");
    }

    #[test]
    fn build_railroad_on_sea_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Sea));

        let result = build_railroad(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Cannot build railroad on sea");
    }

    #[test]
    fn build_railroad_tile_not_found() {
        let mut map = HexMap::new(10, 10);
        let result = build_railroad(&mut map, HexCoord::new(5, 5));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Tile not found");
    }

    // ── build_depot ───────────────────────────────────────────

    #[test]
    fn build_depot_on_valid_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        let result = build_depot(&mut map, coord);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Money::dollars(2000));
        assert!(map.get_tile(coord).unwrap().infrastructure.has_depot);
    }

    #[test]
    fn build_depot_already_exists() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        build_depot(&mut map, coord).unwrap();
        let result = build_depot(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Depot already exists");
    }

    #[test]
    fn build_depot_on_sea_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Sea));

        let result = build_depot(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Cannot build depot on sea");
    }

    // ── build_port ────────────────────────────────────────────

    #[test]
    fn build_port_on_coastal_tile() {
        let mut map = HexMap::new(10, 10);
        let land = HexCoord::new(1, 0);
        let sea = HexCoord::new(2, 0);
        map.set_tile(land, Tile::new(TerrainType::Farm));
        map.set_tile(sea, Tile::new(TerrainType::Sea));

        let result = build_port(&mut map, land);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Money::dollars(3000));
        assert!(map.get_tile(land).unwrap().infrastructure.has_port);
    }

    #[test]
    fn build_port_on_non_coastal_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(5, 5);
        map.set_tile(coord, Tile::new(TerrainType::Farm));
        // No sea neighbors placed

        let result = build_port(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Port must be on a coastal tile");
    }

    #[test]
    fn build_port_on_sea_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Sea));

        let result = build_port(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Cannot build port on sea");
    }

    #[test]
    fn build_port_already_exists() {
        let mut map = HexMap::new(10, 10);
        let land = HexCoord::new(1, 0);
        let sea = HexCoord::new(2, 0);
        map.set_tile(land, Tile::new(TerrainType::Farm));
        map.set_tile(sea, Tile::new(TerrainType::Sea));

        build_port(&mut map, land).unwrap();
        let result = build_port(&mut map, land);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Port already exists");
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
        let mut capital_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        capital_tile.is_capital = true;
        map.set_tile(capital_coord, capital_tile);

        // Middle tile with railroad (province 1)
        let mut mid_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        mid_tile.infrastructure.has_railroad = true;
        map.set_tile(mid_coord, mid_tile);

        // Target tile with depot (province 2)
        let mut target_tile = Tile::with_province(TerrainType::Farm, target_pid);
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

        let mut capital_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        capital_tile.is_capital = true;
        map.set_tile(capital_coord, capital_tile);

        let mut target_tile = Tile::with_province(TerrainType::Farm, target_pid);
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
        let mut capital_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);

        // Sea neighbor for capital (makes it coastal)
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Target tile with port
        let mut target_tile = Tile::with_province(TerrainType::Farm, target_pid);
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
    fn not_connected_capital_has_port_target_does_not() {
        let mut map = HexMap::new(10, 10);
        let capital_coord = HexCoord::new(0, 0);
        let sea_near_capital = HexCoord::new(1, 0);
        let target_coord = HexCoord::new(5, 5);

        let capital_pid = ProvinceId(1);
        let target_pid = ProvinceId(2);

        let mut capital_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Target tile without port
        let target_tile = Tile::with_province(TerrainType::Farm, target_pid);
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
        assert_eq!(fort_cost(1), Ok(Money::dollars(5000)));
        assert_eq!(fort_cost(2), Ok(Money::dollars(7500)));
        assert_eq!(fort_cost(3), Ok(Money::dollars(10000)));
        assert!(fort_cost(4).is_err());
    }

    #[test]
    fn build_fort_level_1() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        let result = build_fort(&mut map, coord);
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
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        build_fort(&mut map, coord).unwrap();
        let (level, cost) = build_fort(&mut map, coord).unwrap();
        assert_eq!(level, 2);
        assert_eq!(cost, Money::dollars(7500));
    }

    #[test]
    fn build_fort_max_level_3() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        build_fort(&mut map, coord).unwrap(); // L1
        build_fort(&mut map, coord).unwrap(); // L2
        build_fort(&mut map, coord).unwrap(); // L3
        let result = build_fort(&mut map, coord);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Fort already at maximum level (3)");
    }

    #[test]
    fn build_fort_on_sea_fails() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(0, 0);
        map.set_tile(coord, Tile::new(TerrainType::Sea));

        let result = build_fort(&mut map, coord);
        assert!(result.is_err());
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

        let mut capital_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        capital_tile.is_capital = true;
        map.set_tile(capital_coord, capital_tile);

        // Gap tile: no railroad, no depot
        let gap_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        map.set_tile(gap_coord, gap_tile);

        // Target tile has depot but is not reachable
        let mut target_tile = Tile::with_province(TerrainType::Farm, target_pid);
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
        let mut capital_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        capital_tile.is_capital = true;
        capital_tile.infrastructure.has_port = true;
        map.set_tile(capital_coord, capital_tile);
        map.set_tile(sea_near_capital, Tile::new(TerrainType::Sea));

        // Railroad chain from capital to province 2 (railroad pathway)
        let mut rr_tile = Tile::with_province(TerrainType::Farm, capital_pid);
        rr_tile.infrastructure.has_railroad = true;
        map.set_tile(railroad_coord, rr_tile);

        let mut depot_tile = Tile::with_province(TerrainType::Farm, railroad_pid);
        depot_tile.infrastructure.has_depot = true;
        map.set_tile(depot_coord, depot_tile);

        // Port province (connected via sea to capital's port)
        let mut port_tile = Tile::with_province(TerrainType::Farm, port_pid);
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
