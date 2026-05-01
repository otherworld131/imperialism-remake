use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::hex::HexCoord;
use crate::map::hex_map::HexMap;
use crate::types::{ProvinceId, TerrainType};

/// Unique identifier for a sea zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeaZoneId(pub u32);

impl std::fmt::Display for SeaZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SZ{}", self.0)
    }
}

/// A sea zone: a named region of sea hexes that fleets can occupy and move through.
///
/// Lakes (enclosed inland bodies of water) are separate zones with `is_lake: true`.
/// Fleets cannot embark from or conduct beachhead operations through lake zones.
#[derive(Debug, Clone)]
pub struct SeaZone {
    pub id: SeaZoneId,
    pub name: String,
    pub hexes: BTreeSet<HexCoord>,
    /// True if this zone is an enclosed inland body of water (not connected to the ocean).
    pub is_lake: bool,
    /// IDs of zones that share a hex-edge boundary with this zone.
    pub adjacent_zone_ids: Vec<SeaZoneId>,
    /// Province IDs whose tiles include at least one hex adjacent to a hex in this zone.
    pub coastal_provinces: Vec<ProvinceId>,
}

impl SeaZone {
    pub fn is_adjacent_to(&self, other: SeaZoneId) -> bool {
        self.adjacent_zone_ids.contains(&other)
    }
}

/// Compute all sea zones from the hex map.
///
/// Algorithm:
/// 1. Flood-fill sea hexes into connected components.
/// 2. Mark components not touching the map border (no neighbor outside the map) as lakes.
/// 3. Subdivide ocean components into quadrant zones (NW/NE/SW/SE).
/// 4. Compute adjacency between all zones.
/// 5. Coastal province assignment is populated separately via `assign_coastal_provinces`.
pub fn compute_sea_zones(hex_map: &HexMap) -> Vec<SeaZone> {
    // Collect all sea hex coordinates
    let all_sea_hexes: HashSet<HexCoord> = hex_map
        .all_tiles()
        .filter(|(_, t)| t.terrain() == TerrainType::Sea)
        .map(|(c, _)| c)
        .collect();

    if all_sea_hexes.is_empty() {
        return Vec::new();
    }

    // Flood-fill into connected components
    let components = flood_fill_components(&all_sea_hexes, hex_map);

    // Classify each component: lake vs ocean
    // A component is ocean if any of its hexes has a neighbor not in the hex_map
    let mut zones: Vec<SeaZone> = Vec::new();
    let mut ocean_hexes: BTreeSet<HexCoord> = BTreeSet::new();
    let mut next_id = 0u32;

    let mut lake_components: Vec<BTreeSet<HexCoord>> = Vec::new();
    let mut ocean_components: Vec<BTreeSet<HexCoord>> = Vec::new();

    for component in components {
        let touches_border = component.iter().any(|&hex| {
            hex.neighbors().iter().any(|n| hex_map.get_tile(*n).is_none())
        });
        if touches_border {
            for h in &component {
                ocean_hexes.insert(*h);
            }
            ocean_components.push(component);
        } else {
            lake_components.push(component);
        }
    }

    // Lakes: each isolated enclosed sea body is one lake zone
    let mut lake_number = 1u32;
    for lake in lake_components {
        zones.push(SeaZone {
            id: SeaZoneId(next_id),
            name: format!("Lake {lake_number}"),
            hexes: lake,
            is_lake: true,
            adjacent_zone_ids: Vec::new(),
            coastal_provinces: Vec::new(),
        });
        next_id += 1;
        lake_number += 1;
    }

    // Ocean: merge all ocean components, then subdivide into a 4-column × 3-row Voronoi grid.
    // Seeds are placed at the median hex of each grid cell; all ocean hexes are assigned to
    // their nearest seed, producing organic (non-rectangular) zone boundaries.
    if !ocean_hexes.is_empty() {
        const GRID_COLS: usize = 4;
        const GRID_ROWS: usize = 3;
        const COL_NAMES: [&str; GRID_COLS] = ["Western", "West-Central", "East-Central", "Eastern"];
        const ROW_NAMES: [&str; GRID_ROWS] = ["Northern", "Central", "Southern"];

        let mut all_ocean_vec: Vec<HexCoord> = ocean_hexes.iter().copied().collect();
        let mut qs: Vec<i32> = all_ocean_vec.iter().map(|h| h.q).collect();
        let mut rs: Vec<i32> = all_ocean_vec.iter().map(|h| h.r).collect();
        qs.sort_unstable();
        rs.sort_unstable();

        // Percentile split points so each band has roughly equal hex count.
        let q_splits: Vec<i32> = (1..GRID_COLS)
            .map(|i| qs[qs.len() * i / GRID_COLS])
            .collect();
        let r_splits: Vec<i32> = (1..GRID_ROWS)
            .map(|i| rs[rs.len() * i / GRID_ROWS])
            .collect();

        // Pick one seed hex per grid cell (median element, sorted by (q,r)).
        let mut seeds: Vec<(usize, HexCoord)> = Vec::new();
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                let mut cell: Vec<HexCoord> = all_ocean_vec.iter().copied()
                    .filter(|h| {
                        q_splits.partition_point(|&s| s <= h.q) == col
                            && r_splits.partition_point(|&s| s <= h.r) == row
                    })
                    .collect();
                if cell.is_empty() {
                    continue;
                }
                cell.sort_unstable_by_key(|h| (h.q, h.r));
                seeds.push((row * GRID_COLS + col, cell[cell.len() / 2]));
            }
        }

        // Voronoi assignment: each hex goes to the zone whose seed is closest.
        all_ocean_vec.sort_unstable_by_key(|h| (h.q, h.r));
        let mut zone_hexes: HashMap<usize, BTreeSet<HexCoord>> = HashMap::new();
        for &hex in &all_ocean_vec {
            let nearest = seeds.iter()
                .min_by_key(|&&(_, seed)| hex_distance(hex, seed))
                .map(|&(zone_idx, _)| zone_idx)
                .unwrap_or(0);
            zone_hexes.entry(nearest).or_default().insert(hex);
        }

        // Emit zones in deterministic order (by zone_idx = row*GRID_COLS+col).
        let grid_id_base = next_id;
        let mut zone_indices: Vec<usize> = zone_hexes.keys().copied().collect();
        zone_indices.sort_unstable();
        for zone_idx in zone_indices {
            if let Some(hexes) = zone_hexes.remove(&zone_idx) {
                let row = zone_idx / GRID_COLS;
                let col = zone_idx % GRID_COLS;
                zones.push(SeaZone {
                    id: SeaZoneId(grid_id_base + zone_idx as u32),
                    name: format!("{} {} Ocean", ROW_NAMES[row], COL_NAMES[col]),
                    hexes,
                    is_lake: false,
                    adjacent_zone_ids: Vec::new(),
                    coastal_provinces: Vec::new(),
                });
            }
        }
        let _ = next_id;
    }

    // Compute adjacency between zones
    compute_zone_adjacency(&mut zones);

    zones
}

/// Populate `coastal_provinces` on each sea zone by checking which province tiles
/// are adjacent to zone hexes. Call after generating provinces.
pub fn assign_coastal_provinces(
    zones: &mut Vec<SeaZone>,
    provinces: &[crate::map::province::Province],
    hex_map: &HexMap,
) {
    // Build a lookup: hex → zone_id for sea hexes
    let mut hex_to_zone: HashMap<HexCoord, SeaZoneId> = HashMap::new();
    for zone in zones.iter() {
        for &hex in &zone.hexes {
            hex_to_zone.insert(hex, zone.id);
        }
    }

    // For each province, find which zones are adjacent (province land tile touches sea zone hex)
    let mut zone_provinces: HashMap<SeaZoneId, Vec<ProvinceId>> = HashMap::new();

    for province in provinces {
        let mut seen_zones: HashSet<SeaZoneId> = HashSet::new();
        for &tile_coord in &province.tiles {
            // ignore tiles not in the map
            if hex_map.get_tile(tile_coord).is_none() {
                continue;
            }
            for neighbor in tile_coord.neighbors() {
                if let Some(&zone_id) = hex_to_zone.get(&neighbor) {
                    seen_zones.insert(zone_id);
                }
            }
        }
        for zone_id in seen_zones {
            zone_provinces.entry(zone_id).or_default().push(province.id);
        }
    }

    for zone in zones.iter_mut() {
        if let Some(pids) = zone_provinces.remove(&zone.id) {
            zone.coastal_provinces = pids;
        }
    }
}

/// Return the sea zone containing the given hex, if any.
pub fn zone_for_hex(zones: &[SeaZone], hex: HexCoord) -> Option<SeaZoneId> {
    zones.iter().find(|z| z.hexes.contains(&hex)).map(|z| z.id)
}

/// Return all non-lake sea zones adjacent to a single hex (the zones containing
/// any sea-tile neighbor of the hex). Used by the port-blockade rule (card #408).
pub fn ocean_zones_adjacent_to_hex(zones: &[SeaZone], hex: HexCoord) -> Vec<SeaZoneId> {
    let mut result: HashSet<SeaZoneId> = HashSet::new();
    for neighbor in hex.neighbors() {
        for zone in zones.iter().filter(|z| !z.is_lake) {
            if zone.hexes.contains(&neighbor) {
                result.insert(zone.id);
            }
        }
    }
    result.into_iter().collect()
}

/// Return all non-lake sea zones adjacent to a given province (touching any of its tiles).
pub fn ocean_zones_adjacent_to_province(
    zones: &[SeaZone],
    province: &crate::map::province::Province,
    hex_map: &HexMap,
) -> Vec<SeaZoneId> {
    let mut hex_to_zone: HashMap<HexCoord, SeaZoneId> = HashMap::new();
    for zone in zones.iter().filter(|z| !z.is_lake) {
        for &hex in &zone.hexes {
            hex_to_zone.insert(hex, zone.id);
        }
    }
    let mut result: HashSet<SeaZoneId> = HashSet::new();
    for &tile in &province.tiles {
        if hex_map.get_tile(tile).is_none() {
            continue;
        }
        for neighbor in tile.neighbors() {
            if let Some(&zid) = hex_to_zone.get(&neighbor) {
                result.insert(zid);
            }
        }
    }
    result.into_iter().collect()
}

// ── Internal helpers ──────────────────────────────────────────────

fn flood_fill_components(
    sea_hexes: &HashSet<HexCoord>,
    _hex_map: &HexMap,
) -> Vec<BTreeSet<HexCoord>> {
    let mut visited: HashSet<HexCoord> = HashSet::new();
    let mut components: Vec<BTreeSet<HexCoord>> = Vec::new();

    for &start in sea_hexes {
        if visited.contains(&start) {
            continue;
        }
        let mut component: BTreeSet<HexCoord> = BTreeSet::new();
        let mut queue: VecDeque<HexCoord> = VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some(hex) = queue.pop_front() {
            component.insert(hex);
            for neighbor in hex.neighbors() {
                if sea_hexes.contains(&neighbor) && !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }
        components.push(component);
    }
    components
}

fn hex_distance(a: HexCoord, b: HexCoord) -> i32 {
    let dq = (a.q - b.q).abs();
    let dr = (a.r - b.r).abs();
    let ds = ((a.q + a.r) - (b.q + b.r)).abs();
    (dq + dr + ds) / 2
}

fn compute_zone_adjacency(zones: &mut Vec<SeaZone>) {
    // Build hex → zone_id lookup
    let mut hex_to_zone: HashMap<HexCoord, usize> = HashMap::new();
    for (idx, zone) in zones.iter().enumerate() {
        for &hex in &zone.hexes {
            hex_to_zone.insert(hex, idx);
        }
    }

    let n = zones.len();
    let mut adjacency: Vec<HashSet<usize>> = vec![HashSet::new(); n];

    for (idx, zone) in zones.iter().enumerate() {
        for &hex in &zone.hexes {
            for neighbor in hex.neighbors() {
                if let Some(&other_idx) = hex_to_zone.get(&neighbor) {
                    if other_idx != idx {
                        adjacency[idx].insert(other_idx);
                        adjacency[other_idx].insert(idx);
                    }
                }
            }
        }
    }

    // Build index → actual SeaZoneId mapping before mutably borrowing zones.
    let index_to_id: Vec<SeaZoneId> = zones.iter().map(|z| z.id).collect();
    for (idx, zone) in zones.iter_mut().enumerate() {
        zone.adjacent_zone_ids = adjacency[idx]
            .iter()
            .map(|&i| index_to_id[i])
            .collect();
        zone.adjacent_zone_ids.sort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::hex_map::HexMap;
    use crate::map::tile::Tile;
    use crate::types::TerrainType;

    fn sea_tile() -> Tile {
        Tile::new(TerrainType::Sea)
    }

    fn land_tile() -> Tile {
        Tile::new(TerrainType::Grassland)
    }

    #[test]
    fn empty_map_produces_no_zones() {
        let map = HexMap::new(10, 10);
        let zones = compute_sea_zones(&map);
        assert!(zones.is_empty());
    }

    #[test]
    fn border_sea_hexes_are_ocean_not_lake() {
        // Single sea hex on the border (has neighbor outside map)
        let mut map = HexMap::new(10, 10);
        // (0,0) will have neighbors like (-1,0) which are outside the map → ocean
        map.set_tile(HexCoord::new(0, 0), sea_tile());
        let zones = compute_sea_zones(&map);
        assert_eq!(zones.len(), 1);
        assert!(!zones[0].is_lake);
    }

    #[test]
    fn enclosed_sea_hex_is_lake() {
        // Create a 5x5 grid of land tiles, with one interior sea tile fully surrounded by land
        let mut map = HexMap::new(10, 10);
        for q in 0..5 {
            for r in 0..5 {
                map.set_tile(HexCoord::new(q, r), land_tile());
            }
        }
        // Replace interior hex with sea — it is surrounded by land on all sides within the map
        let sea_coord = HexCoord::new(2, 2);
        map.set_tile(sea_coord, sea_tile());
        // All 6 neighbors of (2,2) are land tiles, so this sea hex can't reach outside → lake
        let zones = compute_sea_zones(&map);
        assert_eq!(zones.len(), 1);
        assert!(zones[0].is_lake);
    }

    #[test]
    fn ocean_component_split_into_grid() {
        // Create a 6x6 grid of sea hexes all touching the border → all ocean
        let mut map = HexMap::new(6, 6);
        for q in 0..6i32 {
            for r in 0..6i32 {
                map.set_tile(HexCoord::new(q, r), sea_tile());
            }
        }
        let zones = compute_sea_zones(&map);
        // Expect up to 4×3=12 ocean grid zones, no lakes
        let ocean_zones: Vec<_> = zones.iter().filter(|z| !z.is_lake).collect();
        assert!(ocean_zones.len() >= 4, "should produce multiple ocean grid zones, got {}", ocean_zones.len());
        assert!(ocean_zones.len() <= 12, "should produce at most 12 ocean grid zones");
        assert!(zones.iter().all(|z| !z.is_lake));
    }

    #[test]
    fn adjacent_zones_computed_correctly() {
        let mut map = HexMap::new(6, 6);
        for q in 0..6i32 {
            for r in 0..6i32 {
                map.set_tile(HexCoord::new(q, r), sea_tile());
            }
        }
        let zones = compute_sea_zones(&map);
        // All grid zones should be adjacent to at least 1 other zone
        for zone in &zones {
            assert!(!zone.adjacent_zone_ids.is_empty(), "zone {} should have adjacent zones", zone.id);
        }
    }
}
