use std::collections::{HashMap, HashSet};

use crate::hex::HexCoord;
use crate::types::*;

use super::hex_map::HexMap;
use super::province::Province;

// ── Constants ──────────────────────────────────────────────────

const MAP_WIDTH: i32 = 60;
const MAP_HEIGHT: i32 = 40;

const NUM_GREAT_POWERS: usize = 7;
const PROVINCES_PER_GREAT_POWER: usize = 8;
const NUM_MINOR_NATIONS: usize = 16;
const PROVINCES_PER_MINOR_NATION: usize = 4;

const GREAT_POWER_NAMES: [&str; NUM_GREAT_POWERS] = [
    "Deneb", "Devron", "Haxaco", "Kem", "Ordune", "Patagon", "Zimm",
];

const MINOR_NATION_NAMES: [&str; NUM_MINOR_NATIONS] = [
    "Bruhr", "Dedge", "Hurshen", "Idolon", "Issa", "Kathay", "Kessel", "Loke", "Manx", "Pont",
    "Pram", "Sindel", "Twelt", "Wodan", "Zazi", "Zinlu",
];

// ── Seeded RNG (xorshift64) ────────────────────────────────────

struct Rng {
    state: u64,
}

impl Rng {
    fn from_seed(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn range(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            return min;
        }
        let span = (max - min + 1) as u32;
        min + (self.next_u32() % span) as i32
    }

    /// Shuffle a slice in place (Fisher-Yates).
    fn shuffle<T>(&mut self, items: &mut [T]) {
        let len = items.len();
        for i in (1..len).rev() {
            let j = self.next_u32() as usize % (i + 1);
            items.swap(i, j);
        }
    }
}

// ── Output types ───────────────────────────────────────────────

/// Setup data for a single nation produced by the map generator.
#[derive(Debug, Clone)]
pub struct NationSetup {
    pub nation_id: NationId,
    pub name: String,
    pub province_ids: Vec<ProvinceId>,
    pub capital_province: ProvinceId,
}

/// The complete output of map generation.
pub struct GeneratedMap {
    pub hex_map: HexMap,
    pub provinces: Vec<Province>,
    pub great_power_nations: Vec<NationSetup>,
    pub minor_nations: Vec<NationSetup>,
}

// ── Hash utility ───────────────────────────────────────────────

/// DJB2 hash of a string — produces a u64 seed from a map key.
fn djb2_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

// ── Main generator ─────────────────────────────────────────────

/// Generate a complete game map from a seed string.
///
/// The generated map is deterministic: the same `map_key` always produces the
/// same map layout, terrain, provinces, and nation assignments.
pub fn generate_map(map_key: &str) -> GeneratedMap {
    let seed = djb2_hash(map_key);
    let mut rng = Rng::from_seed(seed);

    // Step 1: Create land mask via continent generation
    let land_mask = generate_land_mass(&mut rng);

    // Step 2: Place nation regions and carve provinces
    let total_nations = NUM_GREAT_POWERS + NUM_MINOR_NATIONS;
    let nation_centers = place_nation_centers(&mut rng, &land_mask, total_nations);

    // Step 3: Assign land tiles to nations (Voronoi-like)
    let nation_tiles = assign_tiles_to_nations(&land_mask, &nation_centers);

    // Step 4: Subdivide each nation's territory into provinces
    let mut province_id_counter: u32 = 0;
    let mut all_province_data: Vec<ProvinceData> = Vec::new();
    let mut nation_province_map: Vec<Vec<u32>> = Vec::with_capacity(total_nations);

    for nation_idx in 0..total_nations {
        let num_provinces = if nation_idx < NUM_GREAT_POWERS {
            PROVINCES_PER_GREAT_POWER
        } else {
            PROVINCES_PER_MINOR_NATION
        };

        let mut tiles: Vec<HexCoord> = nation_tiles
            .iter()
            .filter(|&(_, &n)| n == nation_idx)
            .map(|(&coord, _)| coord)
            .collect();
        tiles.sort_by_key(|c| (c.q, c.r));

        let provinces = subdivide_into_provinces(&mut rng, &tiles, num_provinces);
        let mut province_ids = Vec::new();

        for province_tiles in provinces {
            if province_tiles.is_empty() {
                continue;
            }
            let pid = province_id_counter;
            province_id_counter += 1;
            province_ids.push(pid);
            all_province_data.push(ProvinceData {
                id: pid,
                nation_idx,
                tiles: province_tiles,
            });
        }

        // If we didn't get enough provinces (due to small territory), pad with empty ones
        while province_ids.len() < num_provinces {
            let pid = province_id_counter;
            province_id_counter += 1;
            province_ids.push(pid);
            // Give it at least 1 tile from the nation's tiles to avoid empty provinces
            let fallback_tile = tiles.first().copied();
            if let Some(tile) = fallback_tile {
                all_province_data.push(ProvinceData {
                    id: pid,
                    nation_idx,
                    tiles: vec![tile],
                });
            } else {
                // Extremely unlikely: nation has no tiles at all — create a dummy
                all_province_data.push(ProvinceData {
                    id: pid,
                    nation_idx,
                    tiles: vec![HexCoord::new(0, 0)],
                });
            }
        }

        nation_province_map.push(province_ids);
    }

    // Step 5: Build the hex map with terrain
    let mut hex_map = HexMap::new(MAP_WIDTH, MAP_HEIGHT);

    // Place sea tiles for all coordinates
    for q in 0..MAP_WIDTH {
        for r in 0..MAP_HEIGHT {
            let coord = HexCoord::new(q, r);
            hex_map.set_tile(coord, super::tile::Tile::new(TerrainType::Sea));
        }
    }

    // Assign terrain and province to land tiles
    for pdata in &all_province_data {
        let pid = ProvinceId(pdata.id);
        for (i, &coord) in pdata.tiles.iter().enumerate() {
            let terrain = random_land_terrain(&mut rng);
            let mut tile = super::tile::Tile::with_province(terrain, pid);
            if i == 0 {
                tile.is_capital = true;
            }
            hex_map.set_tile(coord, tile);
        }
    }

    // Place hidden mineral deposits on prospectable terrain
    for q in 0..MAP_WIDTH {
        for r in 0..MAP_HEIGHT {
            let coord = HexCoord::new(q, r);
            if let Some(tile) = hex_map.get_tile_mut(coord)
                && tile.terrain().requires_prospecting()
            {
                let roll = rng.range(0, 99);
                if roll < 40 {
                    let deposit = random_mineral_deposit(&mut rng, tile.terrain());
                    tile.reveal_deposit(deposit);
                    // In a real game these would start hidden; for generation
                    // we set them so the data exists. The game would reset
                    // visibility.
                }
            }
        }
    }

    // Step 6: Build Province structs
    let mut provinces: Vec<Province> = Vec::new();

    for pdata in &all_province_data {
        let nation_idx = pdata.nation_idx;
        let nation_name = if nation_idx < NUM_GREAT_POWERS {
            GREAT_POWER_NAMES[nation_idx]
        } else {
            MINOR_NATION_NAMES[nation_idx - NUM_GREAT_POWERS]
        };

        let nation_id = NationId(nation_idx as u32);
        let pid = ProvinceId(pdata.id);
        let capital_tile = pdata.tiles[0];

        let garrison = if nation_idx < NUM_GREAT_POWERS { 4 } else { 3 };

        // First province of a nation is the capital province
        let province_indices_for_nation = &nation_province_map[nation_idx];
        let is_capital_province = province_indices_for_nation
            .first()
            .is_some_and(|&first| first == pdata.id);

        let name = if is_capital_province {
            Province::capital_city_name(nation_name)
        } else {
            format!("{} Province {}", nation_name, pdata.id)
        };

        provinces.push(Province::new(
            pid,
            name,
            nation_id,
            capital_tile,
            pdata.tiles.clone(),
            garrison,
        ));
    }

    // Step 7: Build NationSetup structs
    let mut great_power_nations = Vec::new();
    let mut minor_nations_out = Vec::new();

    for nation_idx in 0..total_nations {
        let nation_id = NationId(nation_idx as u32);
        let name = if nation_idx < NUM_GREAT_POWERS {
            GREAT_POWER_NAMES[nation_idx].to_string()
        } else {
            MINOR_NATION_NAMES[nation_idx - NUM_GREAT_POWERS].to_string()
        };

        let province_ids: Vec<ProvinceId> = nation_province_map[nation_idx]
            .iter()
            .map(|&pid| ProvinceId(pid))
            .collect();

        let capital_province = province_ids[0];

        let setup = NationSetup {
            nation_id,
            name,
            province_ids,
            capital_province,
        };

        if nation_idx < NUM_GREAT_POWERS {
            great_power_nations.push(setup);
        } else {
            minor_nations_out.push(setup);
        }
    }

    GeneratedMap {
        hex_map,
        provinces,
        great_power_nations,
        minor_nations: minor_nations_out,
    }
}

// ── Internal data structures ───────────────────────────────────

struct ProvinceData {
    id: u32,
    nation_idx: usize,
    tiles: Vec<HexCoord>,
}

// ── Land generation ────────────────────────────────────────────

/// Generate a land mask: a set of hex coordinates that are land.
/// Uses continent seeds that grow outward to create landmasses.
fn generate_land_mass(rng: &mut Rng) -> HashSet<HexCoord> {
    let mut land = HashSet::new();

    // Place 5-8 continent seeds spread across the map
    let num_continents = rng.range(5, 8) as usize;
    let mut seeds: Vec<HexCoord> = Vec::new();

    // Divide the map into regions to ensure spread
    for i in 0..num_continents {
        let region_width = MAP_WIDTH / num_continents as i32;
        let q = region_width * i as i32 + rng.range(3, region_width.max(4) - 1);
        let r = rng.range(4, MAP_HEIGHT - 5);
        let seed = HexCoord::new(q.min(MAP_WIDTH - 4), r);
        seeds.push(seed);
    }

    // Grow each continent from its seed
    for seed in &seeds {
        // Target size: 150-350 tiles per continent
        let target_size = rng.range(150, 350) as usize;
        let mut frontier: Vec<HexCoord> = vec![*seed];
        let mut continent: HashSet<HexCoord> = HashSet::new();
        continent.insert(*seed);

        while continent.len() < target_size && !frontier.is_empty() {
            // Pick a random frontier tile
            let idx = rng.next_u32() as usize % frontier.len();
            let current = frontier[idx];

            // Try to expand to neighbors
            let neighbors = current.neighbors();
            for neighbor in &neighbors {
                if continent.contains(neighbor) {
                    continue;
                }
                // Stay within map bounds (with margin)
                if neighbor.q < 1
                    || neighbor.q >= MAP_WIDTH - 1
                    || neighbor.r < 1
                    || neighbor.r >= MAP_HEIGHT - 1
                {
                    continue;
                }
                // Growth probability — decreases with distance from seed for natural shapes
                let dist = seed.distance(*neighbor);
                let prob = if dist < 5 {
                    85
                } else if dist < 10 {
                    65
                } else if dist < 15 {
                    45
                } else {
                    25
                };

                if rng.range(0, 99) < prob {
                    continent.insert(*neighbor);
                    frontier.push(*neighbor);
                    if continent.len() >= target_size {
                        break;
                    }
                }
            }

            // Remove frontier tiles that are fully surrounded
            let all_neighbors_present = current.neighbors().iter().all(|n| continent.contains(n));
            if all_neighbors_present {
                frontier.swap_remove(idx);
            }
        }

        land.extend(continent);
    }

    land
}

/// Place nation centers spread across the land mass.
fn place_nation_centers(rng: &mut Rng, land: &HashSet<HexCoord>, count: usize) -> Vec<HexCoord> {
    let mut land_tiles: Vec<HexCoord> = land.iter().copied().collect();
    // Sort to ensure deterministic iteration regardless of HashSet ordering.
    land_tiles.sort_by_key(|c| (c.q, c.r));
    if land_tiles.is_empty() {
        // Fallback: place centers in a grid pattern
        return (0..count)
            .map(|i| {
                let q = (i as i32 % 8) * 7 + 3;
                let r = (i as i32 / 8) * 10 + 5;
                HexCoord::new(q, r)
            })
            .collect();
    }

    let mut centers: Vec<HexCoord> = Vec::new();
    let min_distance = 6; // Minimum distance between nation centers

    let mut attempts = 0;
    while centers.len() < count && attempts < 5000 {
        attempts += 1;
        let candidate = land_tiles[rng.next_u32() as usize % land_tiles.len()];

        // Check minimum distance from existing centers
        let too_close = centers.iter().any(|c| c.distance(candidate) < min_distance);

        if !too_close {
            centers.push(candidate);
        }
    }

    // If we couldn't place enough, reduce min distance and try again
    while centers.len() < count {
        let candidate = land_tiles[rng.next_u32() as usize % land_tiles.len()];
        let too_close = centers.iter().any(|c| c.distance(candidate) < 3);
        if !too_close {
            centers.push(candidate);
        }
        // Ultimate fallback: just pick any land tile
        if centers.len() < count {
            attempts += 1;
            if attempts > 10000 {
                centers.push(land_tiles[rng.next_u32() as usize % land_tiles.len()]);
            }
        }
    }

    centers
}

/// Assign land tiles to nations using a Voronoi-like partition (nearest center).
fn assign_tiles_to_nations(
    land: &HashSet<HexCoord>,
    centers: &[HexCoord],
) -> HashMap<HexCoord, usize> {
    let mut assignments = HashMap::new();

    for &coord in land {
        let mut best_nation = 0;
        let mut best_dist = i32::MAX;

        for (idx, &center) in centers.iter().enumerate() {
            let dist = coord.distance(center);
            if dist < best_dist {
                best_dist = dist;
                best_nation = idx;
            }
        }

        assignments.insert(coord, best_nation);
    }

    assignments
}

/// Subdivide a nation's tiles into approximately `num_provinces` contiguous groups.
///
/// Uses a simple k-means-like approach: place province seeds, then assign each
/// tile to the nearest seed.
fn subdivide_into_provinces(
    rng: &mut Rng,
    tiles: &[HexCoord],
    num_provinces: usize,
) -> Vec<Vec<HexCoord>> {
    if tiles.is_empty() {
        return vec![vec![]; num_provinces];
    }

    if tiles.len() <= num_provinces {
        // Not enough tiles — give one tile per province
        let mut result: Vec<Vec<HexCoord>> = tiles.iter().map(|&t| vec![t]).collect();
        while result.len() < num_provinces {
            result.push(vec![tiles[0]]);
        }
        return result;
    }

    // Pick province seeds spread through the tile list
    let mut seed_indices: Vec<usize> = Vec::new();
    let mut available: Vec<usize> = (0..tiles.len()).collect();
    rng.shuffle(&mut available);

    // Try to pick well-separated seeds
    for &idx in &available {
        if seed_indices.len() >= num_provinces {
            break;
        }
        let candidate = tiles[idx];
        let too_close = seed_indices
            .iter()
            .any(|&si| tiles[si].distance(candidate) < 2);
        if !too_close {
            seed_indices.push(idx);
        }
    }

    // Fill remaining seeds if needed
    let mut avail_iter = available.iter();
    while seed_indices.len() < num_provinces {
        if let Some(&idx) = avail_iter.next() {
            if !seed_indices.contains(&idx) {
                seed_indices.push(idx);
            }
        } else {
            seed_indices.push(0);
            break;
        }
    }

    // Assign each tile to the nearest seed
    let seeds: Vec<HexCoord> = seed_indices.iter().map(|&i| tiles[i]).collect();
    let mut groups: Vec<Vec<HexCoord>> = vec![vec![]; seeds.len()];

    for &tile in tiles {
        let mut best = 0;
        let mut best_dist = i32::MAX;
        for (i, &seed) in seeds.iter().enumerate() {
            let d = tile.distance(seed);
            if d < best_dist {
                best_dist = d;
                best = i;
            }
        }
        groups[best].push(tile);
    }

    // Ensure no empty groups — steal from the largest
    for i in 0..groups.len() {
        if groups[i].is_empty() {
            // Find the largest group
            let largest = groups
                .iter()
                .enumerate()
                .max_by_key(|(_, g)| g.len())
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            if groups[largest].len() > 1 {
                let tile = groups[largest].pop().unwrap();
                groups[i].push(tile);
            }
        }
    }

    groups
}

// ── Terrain generation ─────────────────────────────────────────

/// Pick a random land terrain type with roughly game-appropriate distribution.
fn random_land_terrain(rng: &mut Rng) -> TerrainType {
    let roll = rng.range(0, 99);
    match roll {
        // Forests ~25%
        0..=12 => TerrainType::HardwoodForest,
        13..=24 => TerrainType::ScrubForest,
        // Farms/orchards/plains ~25%
        25..=34 => TerrainType::Farm,
        35..=40 => TerrainType::Orchard,
        41..=49 => TerrainType::DryPlains,
        // Hills ~15%
        50..=58 => TerrainType::FertileHills,
        59..=64 => TerrainType::BarrenHills,
        // Mountains ~5%
        65..=69 => TerrainType::Mountain,
        // Desert/swamp/tundra ~10%
        70..=73 => TerrainType::Desert,
        74..=77 => TerrainType::Swamp,
        78..=79 => TerrainType::Tundra,
        // Open range/horse ranch/plantation ~10%
        80..=85 => TerrainType::OpenRange,
        86..=90 => TerrainType::HorseRanch,
        91..=96 => TerrainType::Plantation,
        // Remaining: variety
        _ => TerrainType::Farm,
    }
}

/// Pick a mineral deposit type appropriate for the given terrain.
fn random_mineral_deposit(rng: &mut Rng, terrain: TerrainType) -> ResourceType {
    match terrain {
        TerrainType::BarrenHills | TerrainType::Mountain => {
            let roll = rng.range(0, 99);
            match roll {
                0..=34 => ResourceType::Coal,
                35..=64 => ResourceType::Iron,
                65..=84 => ResourceType::Gold,
                _ => ResourceType::Gems,
            }
        }
        TerrainType::Swamp | TerrainType::Desert | TerrainType::Tundra => ResourceType::Oil,
        _ => ResourceType::Coal, // fallback, shouldn't happen
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_map_produces_valid_map() {
        let result = generate_map("test");
        assert!(result.hex_map.tile_count() > 0);
        assert!(!result.provinces.is_empty());
        assert_eq!(result.great_power_nations.len(), NUM_GREAT_POWERS);
        assert_eq!(result.minor_nations.len(), NUM_MINOR_NATIONS);
    }

    #[test]
    fn map_has_correct_number_of_provinces() {
        let result = generate_map("test");
        let expected = NUM_GREAT_POWERS * PROVINCES_PER_GREAT_POWER
            + NUM_MINOR_NATIONS * PROVINCES_PER_MINOR_NATION;
        assert_eq!(result.provinces.len(), expected);
        assert_eq!(result.provinces.len(), 120);
    }

    #[test]
    fn each_great_power_has_8_provinces() {
        let result = generate_map("test");
        for gp in &result.great_power_nations {
            assert_eq!(
                gp.province_ids.len(),
                PROVINCES_PER_GREAT_POWER,
                "Great Power {} should have {} provinces but has {}",
                gp.name,
                PROVINCES_PER_GREAT_POWER,
                gp.province_ids.len()
            );
        }
    }

    #[test]
    fn each_minor_nation_has_4_provinces() {
        let result = generate_map("test");
        for mn in &result.minor_nations {
            assert_eq!(
                mn.province_ids.len(),
                PROVINCES_PER_MINOR_NATION,
                "Minor Nation {} should have {} provinces but has {}",
                mn.name,
                PROVINCES_PER_MINOR_NATION,
                mn.province_ids.len()
            );
        }
    }

    #[test]
    fn generated_map_is_deterministic() {
        let map1 = generate_map("determinism_test");
        let map2 = generate_map("determinism_test");

        assert_eq!(map1.provinces.len(), map2.provinces.len());

        // Check that all provinces have the same tiles
        for (p1, p2) in map1.provinces.iter().zip(map2.provinces.iter()) {
            assert_eq!(p1.id, p2.id);
            assert_eq!(p1.name, p2.name);
            assert_eq!(p1.owner, p2.owner);
            assert_eq!(p1.capital_tile, p2.capital_tile);

            let mut tiles1 = p1.tiles.clone();
            let mut tiles2 = p2.tiles.clone();
            tiles1.sort_by_key(|c| (c.q, c.r));
            tiles2.sort_by_key(|c| (c.q, c.r));
            assert_eq!(tiles1, tiles2);
        }

        // Check nation setups match
        for (n1, n2) in map1
            .great_power_nations
            .iter()
            .zip(map2.great_power_nations.iter())
        {
            assert_eq!(n1.name, n2.name);
            assert_eq!(n1.province_ids, n2.province_ids);
            assert_eq!(n1.capital_province, n2.capital_province);
        }
    }

    #[test]
    fn different_keys_produce_different_maps() {
        let map1 = generate_map("key_alpha");
        let map2 = generate_map("key_beta");

        // The maps should differ in their province capital positions
        let capitals1: Vec<_> = map1.provinces.iter().map(|p| p.capital_tile).collect();
        let capitals2: Vec<_> = map2.provinces.iter().map(|p| p.capital_tile).collect();
        assert_ne!(capitals1, capitals2);
    }

    #[test]
    fn great_power_names_are_correct() {
        let result = generate_map("names_test");
        let names: Vec<&str> = result
            .great_power_nations
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "Deneb", "Devron", "Haxaco", "Kem", "Ordune", "Patagon", "Zimm"
            ]
        );
    }

    #[test]
    fn minor_nation_names_are_correct() {
        let result = generate_map("names_test");
        let names: Vec<&str> = result
            .minor_nations
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "Bruhr", "Dedge", "Hurshen", "Idolon", "Issa", "Kathay", "Kessel", "Loke", "Manx",
                "Pont", "Pram", "Sindel", "Twelt", "Wodan", "Zazi", "Zinlu"
            ]
        );
    }

    #[test]
    fn rng_is_deterministic() {
        let mut rng1 = Rng::from_seed(12345);
        let mut rng2 = Rng::from_seed(12345);
        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn rng_range_stays_in_bounds() {
        let mut rng = Rng::from_seed(42);
        for _ in 0..1000 {
            let val = rng.range(5, 10);
            assert!(val >= 5 && val <= 10, "range produced {val}");
        }
    }

    #[test]
    fn djb2_hash_deterministic() {
        assert_eq!(djb2_hash("test"), djb2_hash("test"));
        assert_ne!(djb2_hash("test"), djb2_hash("other"));
    }

    #[test]
    fn map_has_sea_tiles() {
        let result = generate_map("sea_test");
        let sea_count = result
            .hex_map
            .all_tiles()
            .filter(|(_, t)| t.terrain() == TerrainType::Sea)
            .count();
        assert!(sea_count > 0, "Map should have sea tiles but found none");
    }

    #[test]
    fn map_has_land_tiles() {
        let result = generate_map("land_test");
        let land_count = result
            .hex_map
            .all_tiles()
            .filter(|(_, t)| t.terrain().is_land())
            .count();
        assert!(land_count > 0, "Map should have land tiles but found none");
    }

    #[test]
    fn each_province_has_at_least_one_tile() {
        let result = generate_map("province_tiles_test");
        for province in &result.provinces {
            assert!(
                !province.tiles.is_empty(),
                "Province {} ({}) has no tiles",
                province.id,
                province.name
            );
        }
    }

    #[test]
    fn capital_provinces_have_capital_tiles() {
        let result = generate_map("capital_test");

        for gp in &result.great_power_nations {
            let capital_pid = gp.capital_province;
            let province = result
                .provinces
                .iter()
                .find(|p| p.id == capital_pid)
                .unwrap();
            let capital_coord = province.capital_tile;
            let tile = result.hex_map.get_tile(capital_coord).unwrap();
            assert!(
                tile.is_capital,
                "Capital tile of {} at {} should have is_capital=true",
                gp.name, capital_coord
            );
        }
    }
}
