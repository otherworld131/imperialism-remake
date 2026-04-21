use crate::hex::HexCoord;
use crate::map::HexMap;
use crate::map::Province;
use crate::nation::Nation;
use crate::types::*;

/// Pick a deterministic sea-hex anchor for the given nation's non-beachhead fleet.
///
/// Selection rule (in order):
/// 1. Among every port tile in any coastal province the nation owns, pick the
///    port tile with the lowest `(q, r)`. From that tile, pick the sea neighbor
///    with the lowest `(q, r)`.
/// 2. Otherwise, among every tile in any of the nation's coastal provinces,
///    pick the lowest-`(q, r)` sea neighbor.
/// 3. If the nation owns no coastal province at all, return `None` — a
///    landlocked nation has no place to show a fleet marker and should be
///    skipped.
///
/// The tie-breaking on `(q, r)` matters: the marker location must be stable
/// across frames and across determinism-sensitive code paths.
pub fn fleet_anchor(nation: &Nation, hex_map: &HexMap, provinces: &[Province]) -> Option<HexCoord> {
    // Nation's coastal provinces.
    let owned_coastal: Vec<&Province> = provinces
        .iter()
        .filter(|p| p.owner == nation.id && p.is_coastal())
        .collect();

    // A landlocked nation (no coastal province) never gets a marker.
    if owned_coastal.is_empty() {
        return None;
    }

    // 1. Port-based anchor: collect every port tile in any coastal province,
    //    iterate in ascending `(q, r)` order, and take the first sea neighbor
    //    we find.
    let mut port_tiles: Vec<HexCoord> = owned_coastal
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .filter(|coord| {
            hex_map
                .get_tile(*coord)
                .is_some_and(|t| t.infrastructure.has_port)
        })
        .collect();
    port_tiles.sort_by_key(|c| (c.q, c.r));
    for port in &port_tiles {
        if let Some(sea) = lowest_sea_neighbor(hex_map, *port) {
            return Some(sea);
        }
    }

    // 2. Any coastal-province tile → lowest-`(q, r)` sea neighbor.
    let mut sea_candidates: Vec<HexCoord> = owned_coastal
        .iter()
        .flat_map(|p| p.tiles.iter().copied())
        .flat_map(|c| sea_neighbors(hex_map, c))
        .collect();
    sea_candidates.sort_by_key(|c| (c.q, c.r));
    sea_candidates.into_iter().next()
}

/// Pick a deterministic sea-hex anchor adjacent to the target beachhead province.
///
/// Returns the lowest-`(q, r)` sea neighbor of any tile in `target`.
pub fn beachhead_anchor(hex_map: &HexMap, target: &Province) -> Option<HexCoord> {
    let mut candidates: Vec<HexCoord> = target
        .tiles
        .iter()
        .flat_map(|c| sea_neighbors(hex_map, *c))
        .collect();
    candidates.sort_by_key(|c| (c.q, c.r));
    candidates.into_iter().next()
}

/// Pick a deterministic coast-tile anchor inside the target province (the hex
/// the ships are "landing on"). Used by the frontend to draw a small segment
/// from the beachhead marker toward the shore.
pub fn beachhead_coast_tile(hex_map: &HexMap, target: &Province) -> Option<HexCoord> {
    let mut candidates: Vec<HexCoord> = target
        .tiles
        .iter()
        .copied()
        .filter(|c| !sea_neighbors(hex_map, *c).is_empty())
        .collect();
    candidates.sort_by_key(|c| (c.q, c.r));
    candidates.into_iter().next()
}

fn sea_neighbors(hex_map: &HexMap, coord: HexCoord) -> Vec<HexCoord> {
    coord
        .neighbors()
        .into_iter()
        .filter(|nb| {
            hex_map
                .get_tile(*nb)
                .is_some_and(|t| t.terrain() == TerrainType::Sea)
        })
        .collect()
}

fn lowest_sea_neighbor(hex_map: &HexMap, coord: HexCoord) -> Option<HexCoord> {
    let mut sea = sea_neighbors(hex_map, coord);
    sea.sort_by_key(|c| (c.q, c.r));
    sea.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tile::Tile;
    use crate::nation::{Nation, NationColor};

    fn mk_nation(id: u32, capital: ProvinceId) -> Nation {
        Nation::new(
            NationId(id),
            format!("Nation{id}"),
            NationColor::Red,
            NationType::GreatPower,
            capital,
        )
    }

    fn mk_province(id: u32, owner: u32, capital: HexCoord, tiles: Vec<HexCoord>) -> Province {
        Province::new(
            ProvinceId(id),
            format!("Prov{id}"),
            NationId(owner),
            capital,
            tiles,
            4,
        )
    }

    #[test]
    fn port_anchor_picked_first() {
        let mut map = HexMap::new(10, 10);
        let port_tile = HexCoord::new(2, 2);
        let other_land = HexCoord::new(3, 2);
        let sea_a = HexCoord::new(2, 1);
        let sea_b = HexCoord::new(3, 1);
        map.set_tile(
            port_tile,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(
            other_land,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(sea_a, Tile::new(TerrainType::Sea));
        map.set_tile(sea_b, Tile::new(TerrainType::Sea));
        map.get_tile_mut(port_tile).unwrap().infrastructure.has_port = true;

        let mut prov = mk_province(1, 1, port_tile, vec![port_tile, other_land]);
        prov.coastal = true;
        let nation = mk_nation(1, ProvinceId(1));

        let anchor = fleet_anchor(&nation, &map, &[prov]).expect("anchor");
        // Lowest-(q,r) sea neighbor of the port = (2, 1).
        assert_eq!(anchor, sea_a);
    }

    #[test]
    fn coastal_capital_fallback_when_no_port() {
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0);
        let sea = HexCoord::new(1, 0);
        map.set_tile(
            cap,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(sea, Tile::new(TerrainType::Sea));

        let mut prov = mk_province(1, 1, cap, vec![cap]);
        prov.coastal = true;
        let nation = mk_nation(1, ProvinceId(1));

        let anchor = fleet_anchor(&nation, &map, &[prov]).expect("anchor");
        assert_eq!(anchor, sea);
    }

    #[test]
    fn landlocked_nation_returns_none() {
        // Nation with a single non-coastal province and no port — even if the
        // map has sea far away, we must not place a fleet marker for it.
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0);
        let sea_far = HexCoord::new(5, 0);
        map.set_tile(
            cap,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(sea_far, Tile::new(TerrainType::Sea));

        let prov = mk_province(1, 1, cap, vec![cap]); // not coastal
        let nation = mk_nation(1, ProvinceId(1));

        assert_eq!(fleet_anchor(&nation, &map, &[prov]), None);
    }

    #[test]
    fn non_capital_coastal_province_provides_anchor() {
        // Capital is inland, but a second coastal province exists — fleet
        // marker should anchor off the coastal province.
        let mut map = HexMap::new(10, 10);
        let cap = HexCoord::new(0, 0); // inland
        let coast = HexCoord::new(3, 0); // coastal
        let sea = HexCoord::new(3, -1);
        map.set_tile(
            cap,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(
            coast,
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        map.set_tile(sea, Tile::new(TerrainType::Sea));

        let cap_prov = mk_province(1, 1, cap, vec![cap]);
        let mut coast_prov = mk_province(2, 1, coast, vec![coast]);
        coast_prov.coastal = true;
        let nation = mk_nation(1, ProvinceId(1));

        let anchor = fleet_anchor(&nation, &map, &[cap_prov, coast_prov]).expect("anchor");
        assert_eq!(anchor, sea);
    }

    #[test]
    fn returns_none_when_no_ocean_reachable() {
        let mut map = HexMap::new(5, 5);
        let cap = HexCoord::new(0, 0);
        map.set_tile(
            cap,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );

        let prov = mk_province(1, 1, cap, vec![cap]);
        let nation = mk_nation(1, ProvinceId(1));

        assert_eq!(fleet_anchor(&nation, &map, &[prov]), None);
    }

    #[test]
    fn beachhead_anchor_picks_lowest_sea_neighbor() {
        let mut map = HexMap::new(10, 10);
        let land_a = HexCoord::new(2, 2);
        let land_b = HexCoord::new(3, 2);
        let sea_near_a = HexCoord::new(2, 1);
        let sea_near_b = HexCoord::new(3, 1);
        map.set_tile(
            land_a,
            Tile::with_province(TerrainType::Grassland, ProvinceId(5)),
        );
        map.set_tile(
            land_b,
            Tile::with_province(TerrainType::Grassland, ProvinceId(5)),
        );
        map.set_tile(sea_near_a, Tile::new(TerrainType::Sea));
        map.set_tile(sea_near_b, Tile::new(TerrainType::Sea));

        let target = mk_province(5, 9, land_a, vec![land_a, land_b]);
        let anchor = beachhead_anchor(&map, &target).expect("beachhead anchor");
        assert_eq!(anchor, sea_near_a);
    }

    #[test]
    fn port_tiebreak_lowest_qr() {
        let mut map = HexMap::new(10, 10);
        let port_a = HexCoord::new(2, 2); // lower (q,r)
        let port_b = HexCoord::new(5, 2);
        let sea_a = HexCoord::new(2, 1);
        let sea_b = HexCoord::new(5, 1);
        map.set_tile(
            port_a,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(
            port_b,
            Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );
        map.set_tile(sea_a, Tile::new(TerrainType::Sea));
        map.set_tile(sea_b, Tile::new(TerrainType::Sea));
        map.get_tile_mut(port_a).unwrap().infrastructure.has_port = true;
        map.get_tile_mut(port_b).unwrap().infrastructure.has_port = true;

        let mut prov = mk_province(1, 1, port_a, vec![port_a, port_b]);
        prov.coastal = true;
        let nation = mk_nation(1, ProvinceId(1));

        let anchor = fleet_anchor(&nation, &map, &[prov]).expect("anchor");
        assert_eq!(anchor, sea_a);
    }

    #[test]
    fn beachhead_coast_tile_returns_lowest_coast_tile() {
        let mut map = HexMap::new(10, 10);
        let land_a = HexCoord::new(2, 2);
        let land_b = HexCoord::new(3, 2);
        let inland = HexCoord::new(4, 2);
        let sea_near_a = HexCoord::new(2, 1);
        let sea_near_b = HexCoord::new(3, 1);
        map.set_tile(
            land_a,
            Tile::with_province(TerrainType::Grassland, ProvinceId(5)),
        );
        map.set_tile(
            land_b,
            Tile::with_province(TerrainType::Grassland, ProvinceId(5)),
        );
        map.set_tile(
            inland,
            Tile::with_province(TerrainType::Grassland, ProvinceId(5)),
        );
        map.set_tile(sea_near_a, Tile::new(TerrainType::Sea));
        map.set_tile(sea_near_b, Tile::new(TerrainType::Sea));

        let target = mk_province(5, 9, land_a, vec![land_a, land_b, inland]);
        let coast = beachhead_coast_tile(&map, &target).expect("coast tile");
        // land_a has the lowest (q,r) of the two coastal tiles; inland has no sea neighbor.
        assert_eq!(coast, land_a);
    }
}
