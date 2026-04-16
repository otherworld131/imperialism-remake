use std::collections::{HashMap, HashSet};

use crate::hex::HexCoord;
use crate::types::*;

use super::hex_map::HexMap;
use super::province::Province;

// ── Constants ──────────────────────────────────────────────────

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 50;

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

    // Step 2: Build hex map and assign terrain to ALL land tiles (no provinces yet)
    let mut hex_map = HexMap::new(MAP_WIDTH, MAP_HEIGHT);
    for q in 0..MAP_WIDTH {
        for r in 0..MAP_HEIGHT {
            let coord = HexCoord::new(q, r);
            hex_map.set_tile(coord, super::tile::Tile::new(TerrainType::Sea));
        }
    }
    // Sort land tiles for deterministic RNG consumption (HashSet order is arbitrary)
    let mut land_tiles_sorted: Vec<HexCoord> = land_mask.iter().copied().collect();
    land_tiles_sorted.sort_by_key(|c| (c.q, c.r));
    for &coord in &land_tiles_sorted {
        let (terrain, resource) = random_land_terrain_with_resource(&mut rng);
        if let Some(res) = resource {
            hex_map.set_tile(coord, super::tile::Tile::with_resource(terrain, res));
        } else {
            hex_map.set_tile(coord, super::tile::Tile::new(terrain));
        }
    }

    // Step 2b: Cluster terrain for spatial coherence (forests→patches, mountains→chains, etc.)
    cluster_terrain(&mut hex_map, &land_mask, &mut rng, TerrainType::Forest, 25);
    cluster_terrain(&mut hex_map, &land_mask, &mut rng, TerrainType::Hills, 20);
    cluster_terrain(
        &mut hex_map,
        &land_mask,
        &mut rng,
        TerrainType::Mountain,
        12,
    );
    cluster_terrain(&mut hex_map, &land_mask, &mut rng, TerrainType::Desert, 15);
    cluster_terrain(&mut hex_map, &land_mask, &mut rng, TerrainType::Swamp, 10);

    // Step 3: Cluster food terrain and enforce minimum food (before nations/provinces)
    cluster_food_terrain(&mut hex_map, &land_mask, &mut rng, 15);
    enforce_minimum_food_tiles(&mut hex_map, &land_mask, &mut rng, 20);

    // Step 4: Place nation centers (terrain-aware — prefers food-rich, coastal tiles)
    let total_nations = NUM_GREAT_POWERS + NUM_MINOR_NATIONS;
    let nation_centers = place_nation_centers(&mut rng, &land_mask, total_nations, &hex_map);

    // Step 5: Assign land tiles to nations (weighted Voronoi + normalization)
    let mut nation_tiles = assign_tiles_to_nations(&land_mask, &nation_centers);
    normalize_territory_sizes(&mut nation_tiles, &nation_centers, total_nations);

    // Step 6: Subdivide each nation's territory into provinces
    let mut province_id_counter: u32 = 0;
    let mut all_province_data: Vec<ProvinceData> = Vec::new();
    let mut nation_province_map: Vec<Vec<u32>> = Vec::with_capacity(total_nations);

    #[allow(clippy::needless_range_loop)]
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
            let fallback_tile = tiles.first().copied();
            if let Some(tile) = fallback_tile {
                all_province_data.push(ProvinceData {
                    id: pid,
                    nation_idx,
                    tiles: vec![tile],
                });
            } else {
                // Use nation center as fallback instead of (0,0) which is always Sea
                all_province_data.push(ProvinceData {
                    id: pid,
                    nation_idx,
                    tiles: vec![nation_centers[nation_idx]],
                });
            }
        }

        nation_province_map.push(province_ids);
    }

    // Step 7: Stamp province IDs onto existing terrain tiles
    for pdata in &all_province_data {
        let pid = ProvinceId(pdata.id);
        for &coord in &pdata.tiles {
            if let Some(tile) = hex_map.get_tile_mut(coord) {
                tile.province_id = Some(pid);
            }
        }
    }

    // Step 8: Select country capitals (best tile across ALL nation tiles)
    // This also reorders nation_province_map so capital province is first.
    select_country_capitals(&mut hex_map, &all_province_data, &mut nation_province_map);

    // Step 9: Select province capitals for remaining (non-capital) provinces
    select_province_capitals(&mut hex_map, &all_province_data);

    // Step 10: Place hidden mineral deposits on prospectable terrain
    for q in 0..MAP_WIDTH {
        for r in 0..MAP_HEIGHT {
            let coord = HexCoord::new(q, r);
            if let Some(tile) = hex_map.get_tile_mut(coord)
                && tile.terrain().can_have_deposits()
                && tile.resource_deposit().is_none()
            {
                let roll = rng.range(0, 99);
                if roll < 40 {
                    let deposit = random_mineral_deposit(&mut rng, tile.terrain());
                    tile.set_resource(deposit); // set_resource, NOT reveal — keeps prospected=false
                }
            }
        }
    }

    // Step 11: Build Province structs
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
        let capital_tile = pdata
            .tiles
            .iter()
            .find(|&&coord| {
                hex_map
                    .get_tile(coord)
                    .map(|t| t.is_capital)
                    .unwrap_or(false)
            })
            .copied()
            .unwrap_or(pdata.tiles[0]);

        let garrison = if nation_idx < NUM_GREAT_POWERS { 4 } else { 3 };

        // Capital province is first in nation_province_map (set by select_country_capitals)
        let province_indices_for_nation = &nation_province_map[nation_idx];
        let is_capital_province = province_indices_for_nation
            .first()
            .is_some_and(|&first| first == pdata.id);

        let name = if is_capital_province {
            Province::capital_city_name(nation_name)
        } else {
            format!("{} Province {}", nation_name, pdata.id)
        };

        let mut prov = Province::new(
            pid,
            name,
            nation_id,
            capital_tile,
            pdata.tiles.clone(),
            garrison,
        );
        prov.coastal = super::province::compute_coastal(&hex_map, &prov);
        provinces.push(prov);
    }

    // Step 12: Build NationSetup structs
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

/// Validate that a generated map satisfies game invariants.
///
/// Checks:
/// - Province count: 7 * 8 + 16 * 4 = 120
/// - Each Great Power has exactly 8 provinces
/// - Each Minor Nation has exactly 4 provinces
/// - Every province has at least 1 tile
/// - Every province capital tile exists in the map
pub fn validate_map(map: &GeneratedMap) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Check province count: should be 7*8 + 16*4 = 120
    let expected = NUM_GREAT_POWERS * PROVINCES_PER_GREAT_POWER
        + NUM_MINOR_NATIONS * PROVINCES_PER_MINOR_NATION;
    if map.provinces.len() != expected {
        errors.push(format!(
            "Expected {} provinces, got {}",
            expected,
            map.provinces.len()
        ));
    }

    // Check each GP has 8 provinces
    for gp in &map.great_power_nations {
        if gp.province_ids.len() != PROVINCES_PER_GREAT_POWER {
            errors.push(format!(
                "{} has {} provinces, expected {}",
                gp.name,
                gp.province_ids.len(),
                PROVINCES_PER_GREAT_POWER
            ));
        }
    }

    // Check each MN has 4 provinces
    for mn in &map.minor_nations {
        if mn.province_ids.len() != PROVINCES_PER_MINOR_NATION {
            errors.push(format!(
                "{} has {} provinces, expected {}",
                mn.name,
                mn.province_ids.len(),
                PROVINCES_PER_MINOR_NATION
            ));
        }
    }

    // Check all provinces have at least 1 tile
    for prov in &map.provinces {
        if prov.tiles.is_empty() {
            errors.push(format!("Province {} has no tiles", prov.name));
        }
    }

    // Check all province capital tiles exist in the map
    for prov in &map.provinces {
        if map.hex_map.get_tile(prov.capital_tile).is_none() {
            errors.push(format!("Province {} capital tile not in map", prov.name));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
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
        let target_size = rng.range(200, 500) as usize;
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

/// Place nation centers spread across the land mass, preferring terrain-rich locations.
fn place_nation_centers(
    rng: &mut Rng,
    land: &HashSet<HexCoord>,
    count: usize,
    hex_map: &HexMap,
) -> Vec<HexCoord> {
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

    // Score all land tiles by terrain quality, then sort best-first
    let mut scored_tiles: Vec<(HexCoord, u32)> = land_tiles
        .iter()
        .map(|&coord| (coord, score_nation_center(coord, hex_map)))
        .collect();
    scored_tiles.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(a.0.q.cmp(&b.0.q))
            .then(a.0.r.cmp(&b.0.r))
    });

    let mut centers: Vec<HexCoord> = Vec::new();
    let min_distance = 8;

    // Pick top-scoring tiles that satisfy min distance
    for &(candidate, _) in &scored_tiles {
        if centers.len() >= count {
            break;
        }
        let too_close = centers.iter().any(|c| c.distance(candidate) < min_distance);
        if !too_close {
            centers.push(candidate);
        }
    }

    // Fallback with reduced distance if needed (iteration-limited to prevent infinite loop)
    let mut fallback_attempts = 0;
    while centers.len() < count && fallback_attempts < 2000 {
        fallback_attempts += 1;
        let candidate = land_tiles[rng.next_u32() as usize % land_tiles.len()];
        let too_close = centers.iter().any(|c| c.distance(candidate) < 4);
        if !too_close {
            centers.push(candidate);
        }
        // Ultimate fallback: just pick any land tile (allow duplicates after many attempts)
        if centers.len() < count && fallback_attempts > 1000 {
            let candidate2 = land_tiles[rng.next_u32() as usize % land_tiles.len()];
            centers.push(candidate2);
        } else if centers.len() < count {
            let candidate2 = land_tiles[rng.next_u32() as usize % land_tiles.len()];
            if !centers.contains(&candidate2) {
                centers.push(candidate2);
            }
        }
    }

    centers
}

/// Assign land tiles to nations using a weighted Voronoi partition.
/// Minor nation distances are scaled by √2 so great powers claim ~2× territory.
fn assign_tiles_to_nations(
    land: &HashSet<HexCoord>,
    centers: &[HexCoord],
) -> HashMap<HexCoord, usize> {
    let mut assignments = HashMap::new();

    for &coord in land {
        let mut best_nation = 0;
        let mut best_dist = f64::MAX;

        for (idx, &center) in centers.iter().enumerate() {
            let raw_dist = coord.distance(center) as f64;
            // Minor nations have inflated distance → claim less territory
            let effective_dist = if idx >= NUM_GREAT_POWERS {
                raw_dist * std::f64::consts::SQRT_2
            } else {
                raw_dist
            };
            if effective_dist < best_dist {
                best_dist = effective_dist;
                best_nation = idx;
            }
        }

        assignments.insert(coord, best_nation);
    }

    assignments
}

/// Normalize territory sizes so GPs have ~2x the tiles of MNs.
///
/// After the weighted Voronoi, some nations may be too large or too small due to
/// coastlines and center placement. This iteratively reassigns border tiles from
/// oversized nations to undersized adjacent nations until the 2:1 ratio is met.
fn normalize_territory_sizes(
    assignments: &mut HashMap<HexCoord, usize>,
    _centers: &[HexCoord],
    total_nations: usize,
) {
    let total_land = assignments.len();
    // Target: GP gets 2 shares, MN gets 1 share
    // Total shares = 7*2 + 16*1 = 30
    let total_shares = NUM_GREAT_POWERS * 2 + (total_nations - NUM_GREAT_POWERS);
    let share_size = total_land as f64 / total_shares as f64;

    let target_size = |nation_idx: usize| -> usize {
        if nation_idx < NUM_GREAT_POWERS {
            (share_size * 2.0) as usize
        } else {
            share_size as usize
        }
    };

    // Run normalization passes
    for _ in 0..20 {
        // Count current sizes
        let mut sizes = vec![0usize; total_nations];
        for &nation in assignments.values() {
            sizes[nation] += 1;
        }

        // Find oversized nations (>120% of target) and their border tiles
        let mut changed = false;
        let mut border_tiles: Vec<(HexCoord, usize, usize)> = Vec::new(); // (coord, from, to)

        // Collect all tiles sorted for determinism
        let mut all_tiles: Vec<(HexCoord, usize)> =
            assignments.iter().map(|(&c, &n)| (c, n)).collect();
        all_tiles.sort_by_key(|(c, _)| (c.q, c.r));

        for &(coord, nation) in &all_tiles {
            let target = target_size(nation);
            if sizes[nation] <= target {
                continue; // not oversized
            }

            // Check if this tile borders a different, undersized nation
            for neighbor in coord.neighbors() {
                if let Some(&neighbor_nation) = assignments.get(&neighbor)
                    && neighbor_nation != nation
                {
                    let neighbor_target = target_size(neighbor_nation);
                    if sizes[neighbor_nation] < neighbor_target {
                        // Don't reassign if it would make the source too small
                        if sizes[nation] > target {
                            border_tiles.push((coord, nation, neighbor_nation));
                            break;
                        }
                    }
                }
            }
        }

        for (coord, from, to) in &border_tiles {
            let from_target = target_size(*from);
            let to_target = target_size(*to);
            if sizes[*from] > from_target && sizes[*to] < to_target {
                assignments.insert(*coord, *to);
                sizes[*from] -= 1;
                sizes[*to] += 1;
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }
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

// ── Food tile guarantee ───────────────────────────────────────

fn is_food_terrain(tile: &super::tile::Tile) -> bool {
    matches!(
        tile.resource_deposit(),
        Some(ResourceType::Grain | ResourceType::Fruit | ResourceType::Livestock)
    )
}

/// Whether this tile's terrain is replaceable when adding food tiles (least valuable first).
fn is_replaceable_for_food(terrain: TerrainType) -> bool {
    matches!(
        terrain,
        TerrainType::Desert
            | TerrainType::Tundra
            | TerrainType::Swamp
            | TerrainType::Hills
            | TerrainType::Mountain
            | TerrainType::Grassland
    )
}

/// Ensure at least `min_percent`% of land tiles produce food.
/// Replaces barren/desert/mountain tiles with random food terrain if needed.
/// Called before provinces exist — works on raw land tiles.
fn enforce_minimum_food_tiles(
    hex_map: &mut super::hex_map::HexMap,
    land_mask: &HashSet<HexCoord>,
    rng: &mut Rng,
    min_percent: usize,
) {
    // Sort for deterministic RNG consumption
    let mut all_land: Vec<HexCoord> = land_mask.iter().copied().collect();
    all_land.sort_by_key(|c| (c.q, c.r));

    if all_land.is_empty() {
        return;
    }

    let food_count = all_land
        .iter()
        .filter(|&&coord| {
            hex_map
                .get_tile(coord)
                .map(is_food_terrain)
                .unwrap_or(false)
        })
        .count();

    let min_food = all_land.len() * min_percent / 100;

    if food_count >= min_food {
        return;
    }

    let needed = min_food - food_count;

    // Collect replaceable non-food tiles (no capital check needed — none exist yet)
    let mut replaceable: Vec<HexCoord> = all_land
        .iter()
        .filter(|&&coord| {
            hex_map
                .get_tile(coord)
                .map(|t| is_replaceable_for_food(t.terrain()))
                .unwrap_or(false)
        })
        .copied()
        .collect();

    // Fisher-Yates shuffle for even distribution (range is inclusive, so use i not i+1)
    for i in (1..replaceable.len()).rev() {
        let j = rng.range(0, i as i32) as usize;
        replaceable.swap(i, j);
    }

    for &coord in replaceable.iter().take(needed) {
        let (food_terrain, food_resource) = match rng.range(0, 3) as u32 {
            0 => (TerrainType::Grassland, ResourceType::Grain),
            1 => (TerrainType::Grassland, ResourceType::Fruit),
            _ => (TerrainType::Grassland, ResourceType::Livestock),
        };
        hex_map.set_tile(
            coord,
            super::tile::Tile::with_resource(food_terrain, food_resource),
        );
    }
}

// ── Food clustering ──────────────────────────────────────────

/// Spread food tiles to adjacent tiles to create natural clusters (farm belts, pasture regions).
/// Cluster a terrain type by spreading it to adjacent bare Grassland tiles.
/// Each tile of `target_terrain` has `chance_percent`% chance to convert each
/// bare Grassland neighbor to the same terrain (no resource).
/// Called before provinces exist.
fn cluster_terrain(
    hex_map: &mut super::hex_map::HexMap,
    _land_mask: &HashSet<HexCoord>,
    rng: &mut Rng,
    target_terrain: TerrainType,
    chance_percent: i32,
) {
    // Snapshot all tiles of the target terrain
    let mut source_tiles: Vec<HexCoord> = Vec::new();
    for q in 0..MAP_WIDTH {
        for r in 0..MAP_HEIGHT {
            let coord = HexCoord::new(q, r);
            if let Some(tile) = hex_map.get_tile(coord)
                && tile.terrain() == target_terrain
            {
                source_tiles.push(coord);
            }
        }
    }

    // For each source tile, chance to spread to bare Grassland neighbors
    for coord in &source_tiles {
        for neighbor in coord.neighbors() {
            if rng.range(0, 100) >= chance_percent {
                continue;
            }
            // Only convert bare Grassland (no resource) to preserve resource distribution
            if let Some(tile) = hex_map.get_tile(neighbor)
                && tile.terrain() == TerrainType::Grassland
                && tile.resource_deposit().is_none()
            {
                hex_map.set_tile(neighbor, super::tile::Tile::new(target_terrain));
            }
        }
    }
}

/// Each food tile has `chance_percent`% chance to convert each replaceable neighbor to the same type.
/// Called before provinces exist — works on raw land tiles (no province_id, no capitals).
fn cluster_food_terrain(
    hex_map: &mut super::hex_map::HexMap,
    _land_mask: &HashSet<HexCoord>,
    rng: &mut Rng,
    chance_percent: i32,
) {
    // Collect all current food tiles (snapshot before mutation)
    let mut food_tiles: Vec<(HexCoord, TerrainType, Option<ResourceType>)> = Vec::new();
    for q in 0..MAP_WIDTH {
        for r in 0..MAP_HEIGHT {
            let coord = HexCoord::new(q, r);
            if let Some(tile) = hex_map.get_tile(coord)
                && is_food_terrain(tile)
            {
                food_tiles.push((coord, tile.terrain(), tile.resource_deposit()));
            }
        }
    }

    // For each food tile, chance to spread to replaceable neighbors
    for (coord, terrain, resource) in &food_tiles {
        for neighbor in coord.neighbors() {
            if rng.range(0, 100) >= chance_percent {
                continue;
            }
            // Only replace truly barren terrain without existing resources
            if let Some(tile) = hex_map.get_tile(neighbor)
                && tile.resource_deposit().is_none()
                && matches!(
                    tile.terrain(),
                    TerrainType::Desert
                        | TerrainType::Tundra
                        | TerrainType::Swamp
                        | TerrainType::Grassland
                )
            {
                let mut new_tile = super::tile::Tile::new(*terrain);
                if let Some(res) = resource {
                    new_tile.set_resource(*res);
                }
                hex_map.set_tile(neighbor, new_tile);
            }
        }
    }
}

// ── Nation center & capital scoring ─────────────────────────────

/// Score a tile as a potential nation center.
/// Prefers food-rich, coastal tiles; penalizes barren terrain.
fn score_nation_center(coord: HexCoord, hex_map: &HexMap) -> u32 {
    let mut score: u32 = 0;

    if let Some(tile) = hex_map.get_tile(coord) {
        if is_food_terrain(tile) {
            score += 10;
        }
        if matches!(
            tile.terrain(),
            TerrainType::Desert | TerrainType::Tundra | TerrainType::Mountain | TerrainType::Swamp
        ) && tile.resource_deposit().is_none()
        {
            return 0;
        }
    }

    // Coastal bonus
    let is_coastal = coord.neighbors().iter().any(|n| {
        hex_map
            .get_tile(*n)
            .map(|t| t.terrain() == TerrainType::Sea)
            .unwrap_or(false)
    });
    if is_coastal {
        score += 15;
    }

    // Food tiles within 2 hexes
    for nearby in coord.range(2) {
        if let Some(tile) = hex_map.get_tile(nearby)
            && is_food_terrain(tile)
        {
            score += 3;
        }
    }

    score
}

/// Score a tile as a potential capital location.
/// Prefers coastal tiles with nearby food.
fn score_capital_candidate(coord: HexCoord, hex_map: &super::hex_map::HexMap) -> u32 {
    let mut score: u32 = 0;

    // Coastal bonus: adjacent to sea tile
    let is_coastal = coord.neighbors().iter().any(|n| {
        hex_map
            .get_tile(*n)
            .map(|t| t.terrain() == TerrainType::Sea)
            .unwrap_or(false)
    });
    if is_coastal {
        score += 20;
    }

    // Food tiles within 2 hexes
    for nearby in coord.range(2) {
        if let Some(tile) = hex_map.get_tile(nearby)
            && is_food_terrain(tile)
        {
            score += 5;
        }
    }

    // Penalty for being on undesirable terrain without resources
    if let Some(tile) = hex_map.get_tile(coord)
        && matches!(
            tile.terrain(),
            TerrainType::Desert | TerrainType::Tundra | TerrainType::Swamp | TerrainType::Mountain
        )
        && tile.resource_deposit().is_none()
    {
        score = score.saturating_sub(15);
    }

    score
}

/// Select country capitals for all nations.
///
/// For each nation, scores ALL tiles across ALL its provinces and picks the
/// best one as the national capital. The province containing that tile is
/// reordered to be first in `nation_province_map` (making it the capital province).
fn select_country_capitals(
    hex_map: &mut super::hex_map::HexMap,
    province_data: &[ProvinceData],
    nation_province_map: &mut [Vec<u32>],
) {
    let total_nations = NUM_GREAT_POWERS + NUM_MINOR_NATIONS;
    for nation_idx in 0..total_nations {
        if nation_idx >= nation_province_map.len() {
            continue;
        }
        let nation_pids = nation_province_map[nation_idx].clone();

        let mut best_coord: Option<HexCoord> = None;
        let mut best_score: u32 = 0;
        let mut best_province_id: u32 = nation_pids[0];

        // Score ALL tiles across ALL provinces of this nation
        for &pid in &nation_pids {
            if let Some(pdata) = province_data.iter().find(|p| p.id == pid) {
                for &coord in &pdata.tiles {
                    let s = score_capital_candidate(coord, hex_map);
                    if s > best_score || best_coord.is_none() {
                        best_score = s;
                        best_coord = Some(coord);
                        best_province_id = pid;
                    }
                }
            }
        }

        if let Some(capital) = best_coord
            && let Some(tile) = hex_map.get_tile_mut(capital)
        {
            tile.is_capital = true;
        }

        // Reorder so capital province is first
        let pids = &mut nation_province_map[nation_idx];
        if let Some(pos) = pids.iter().position(|&id| id == best_province_id) {
            pids.swap(0, pos);
        }
    }
}

/// Select province capitals for provinces that don't already have one.
///
/// Called after `select_country_capitals` — the capital province already has
/// its capital tile set, so this only handles the remaining provinces.
fn select_province_capitals(hex_map: &mut super::hex_map::HexMap, province_data: &[ProvinceData]) {
    for pdata in province_data {
        // Skip if this province already has a capital (set by country capital selection)
        let has_capital = pdata.tiles.iter().any(|&coord| {
            hex_map
                .get_tile(coord)
                .map(|t| t.is_capital)
                .unwrap_or(false)
        });
        if has_capital {
            continue;
        }

        let mut best_coord: Option<HexCoord> = None;
        let mut best_score: u32 = 0;
        for &coord in &pdata.tiles {
            let s = score_capital_candidate(coord, hex_map);
            if s > best_score || best_coord.is_none() {
                best_score = s;
                best_coord = Some(coord);
            }
        }
        if let Some(capital) = best_coord
            && let Some(tile) = hex_map.get_tile_mut(capital)
        {
            tile.is_capital = true;
        }
    }
}

// ── Terrain generation ─────────────────────────────────────────

/// Pick a random land terrain type and optional resource with game-appropriate distribution.
/// Returns (terrain, optional resource).
/// Pick a random terrain + optional resource. ~60% of tiles have no resource.
fn random_land_terrain_with_resource(rng: &mut Rng) -> (TerrainType, Option<ResourceType>) {
    let roll = rng.range(0, 99);
    match roll {
        // ── No resource (~60%) ────────────────────────────────
        // Bare grassland ~30%
        0..=29 => (TerrainType::Grassland, None),
        // Bare forest ~10%
        30..=39 => (TerrainType::Forest, None),
        // Bare hills ~8%
        40..=47 => (TerrainType::Hills, None),
        // Mountains ~5% (deposits placed separately via prospecting)
        48..=52 => (TerrainType::Mountain, None),
        // Desert/swamp/tundra ~7% (oil deposits placed separately)
        53..=55 => (TerrainType::Desert, None),
        56..=58 => (TerrainType::Swamp, None),
        59 => (TerrainType::Tundra, None),

        // ── With resource (~40%) ──────────────────────────────
        // Grassland + food/cash crops
        60..=65 => (TerrainType::Grassland, Some(ResourceType::Grain)),
        66..=69 => (TerrainType::Grassland, Some(ResourceType::Fruit)),
        70..=74 => (TerrainType::Grassland, Some(ResourceType::Cotton)),
        75..=78 => (TerrainType::Grassland, Some(ResourceType::Livestock)),
        79..=81 => (TerrainType::Grassland, Some(ResourceType::Horses)),
        // Hills + wool
        82..=86 => (TerrainType::Hills, Some(ResourceType::Wool)),
        // Forest + timber
        87..=96 => (TerrainType::Forest, Some(ResourceType::Timber)),
        // Remaining: grain
        _ => (TerrainType::Grassland, Some(ResourceType::Grain)),
    }
}

/// Pick a mineral deposit type appropriate for the given terrain.
fn random_mineral_deposit(rng: &mut Rng, terrain: TerrainType) -> ResourceType {
    match terrain {
        TerrainType::Hills | TerrainType::Mountain => {
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
            assert!((5..=10).contains(&val), "range produced {val}");
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

    #[test]
    fn validate_map_passes_for_default_key() {
        let result = generate_map("test");
        let validation = validate_map(&result);
        assert!(
            validation.is_ok(),
            "validate_map failed: {:?}",
            validation.err()
        );
    }

    #[test]
    fn validate_map_passes_for_multiple_keys() {
        let keys = ["alpha", "beta", "gamma", "delta", "epsilon"];
        for key in &keys {
            let result = generate_map(key);
            let validation = validate_map(&result);
            assert!(
                validation.is_ok(),
                "validate_map failed for key '{}': {:?}",
                key,
                validation.err()
            );
        }
    }
}
