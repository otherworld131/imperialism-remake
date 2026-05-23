use std::collections::{HashMap, HashSet};

use crate::hex::HexCoord;
use crate::types::*;

use super::bounds::MapBounds;
use super::hex_map::HexMap;
use super::province::Province;

/// Default hard margin: continents are never placed in this many cells of
/// the map's edge. A small absolute floor only — the natural sea ring comes
/// from the soft falloff. Tunable via `TerrainMix::sea_hard_margin`.
pub const DEFAULT_SEA_HARD_MARGIN: i32 = 1;

/// Default cells of soft falloff around the map edge. Continent growth
/// probability decays linearly from full strength at distance ≥ this many
/// cells down to 0 at the hard margin. Tunable via
/// `TerrainMix::sea_falloff_radius`.
pub const DEFAULT_SEA_FALLOFF_RADIUS: i32 = 5;

// ── Constants ──────────────────────────────────────────────────

pub const DEFAULT_MAP_WIDTH: i32 = 80;
pub const DEFAULT_MAP_HEIGHT: i32 = 50;
pub const DEFAULT_NUM_GREAT_POWERS: usize = 7;
pub const DEFAULT_NUM_MINOR_NATIONS: usize = 16;

const PROVINCES_PER_GREAT_POWER: usize = 8;
const PROVINCES_PER_MINOR_NATION: usize = 4;

const GREAT_POWER_NAME_POOL: [&str; 7] = [
    "Deneb", "Devron", "Haxaco", "Kem", "Ordune", "Patagon", "Zimm",
];

const MINOR_NATION_NAME_POOL: [&str; 16] = [
    "Bruhr", "Dedge", "Hurshen", "Idolon", "Issa", "Kathay", "Kessel", "Loke", "Manx", "Pont",
    "Pram", "Sindel", "Twelt", "Wodan", "Zazi", "Zinlu",
];

/// Per-terrain weights and clustering parameters for map generation.
///
/// Weights are *relative* — the picker normalizes them, so values of
/// (10, 5, 1, 1, 1, 1) are equivalent to (100, 50, 10, 10, 10, 10).
/// The defaults reproduce the historical hardcoded distribution.
#[derive(Debug, Clone, Copy)]
pub struct TerrainMix {
    pub grassland: f32,
    pub forest: f32,
    pub hills: f32,
    pub mountain: f32,
    pub desert: f32,
    pub swamp: f32,
    pub tundra: f32,
    /// Clustering spread chance for each terrain (0–100). Each cluster source
    /// tile rolls for every neighbor; higher values produce larger contiguous
    /// patches.
    pub forest_cluster: i32,
    pub hills_cluster: i32,
    pub mountain_cluster: i32,
    pub desert_cluster: i32,
    pub swamp_cluster: i32,
    /// 0 = tundra is distributed uniformly over land. 1 = tundra weight is
    /// strongly biased toward the top and bottom rows of the map (the poles).
    pub pole_tundra_strength: f32,
    /// Outermost guaranteed-sea ring width (cells). Continents never spawn in
    /// this perimeter so fleets can always circumnavigate.
    pub sea_hard_margin: i32,
    /// Soft-falloff zone width (cells). Continent growth probability decays
    /// linearly from full strength at this distance from the edge to 0 at
    /// the hard margin, producing organic coastlines along the perimeter
    /// instead of a rectangular cut. Must be > sea_hard_margin.
    pub sea_falloff_radius: i32,
    /// Multiplier on continent target size: 1.0 = baseline (~200-500 tiles
    /// per continent), 0.3 = sparse archipelagos, 2.0 = dense supercontinents.
    pub land_amount: f32,
    /// Percent of eligible mountain headwaters that spawn a river.
    pub river_source_percent: i32,
}

impl Default for TerrainMix {
    fn default() -> Self {
        // Historical pre-cluster baseline (per-terrain percent of land tiles
        // before resources are rolled and before clustering spreads things).
        Self {
            grassland: 55.0,
            forest: 20.0,
            hills: 13.0,
            mountain: 5.0,
            desert: 3.0,
            swamp: 3.0,
            tundra: 1.0,
            forest_cluster: 25,
            hills_cluster: 20,
            mountain_cluster: 12,
            desert_cluster: 15,
            swamp_cluster: 10,
            pole_tundra_strength: 0.5,
            sea_hard_margin: DEFAULT_SEA_HARD_MARGIN,
            sea_falloff_radius: DEFAULT_SEA_FALLOFF_RADIUS,
            land_amount: 1.0,
            river_source_percent: 20,
        }
    }
}

/// Configuration for map generation. Use `MapGenConfig::default()` for canonical
/// 80×50 maps with 7 great powers and 16 minor nations.
#[derive(Debug, Clone, Copy)]
pub struct MapGenConfig {
    pub width: i32,
    pub height: i32,
    pub num_great_powers: usize,
    pub num_minor_nations: usize,
    pub terrain: TerrainMix,
}

impl Default for MapGenConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            num_great_powers: DEFAULT_NUM_GREAT_POWERS,
            num_minor_nations: DEFAULT_NUM_MINOR_NATIONS,
            terrain: TerrainMix::default(),
        }
    }
}

/// Name for great power at index `i`. Cycles the pool with a `-N` suffix when `i >= pool.len()`.
fn great_power_name(i: usize) -> String {
    let pool = &GREAT_POWER_NAME_POOL;
    let base = pool[i % pool.len()];
    let suffix = i / pool.len();
    if suffix == 0 {
        base.to_string()
    } else {
        format!("{}-{}", base, suffix + 1)
    }
}

/// Name for minor nation at offset `i` within the minors block (0 = first minor).
fn minor_nation_name(i: usize) -> String {
    let pool = &MINOR_NATION_NAME_POOL;
    let base = pool[i % pool.len()];
    let suffix = i / pool.len();
    if suffix == 0 {
        base.to_string()
    } else {
        format!("{}-{}", base, suffix + 1)
    }
}

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
    pub sea_zones: Vec<super::sea_zones::SeaZone>,
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

/// Generate a canonical map (80×50, 7 GPs, 16 minors). Equivalent to
/// `generate_map_with_config(map_key, &MapGenConfig::default())`.
pub fn generate_map(map_key: &str) -> GeneratedMap {
    generate_map_with_config(map_key, &MapGenConfig::default())
}

/// Generate a complete game map from a seed string and a config.
///
/// The generated map is deterministic: the same `map_key` + `cfg` always produces the
/// same map layout, terrain, provinces, and nation assignments.
pub fn generate_map_with_config(map_key: &str, cfg: &MapGenConfig) -> GeneratedMap {
    let seed = djb2_hash(map_key);
    let mut rng = Rng::from_seed(seed);
    let map_width = cfg.width;
    let map_height = cfg.height;
    let num_gp = cfg.num_great_powers;
    let num_mn = cfg.num_minor_nations;

    // Step 1: Create land mask via continent generation (margin enforced)
    let bounds = MapBounds::new(map_width, map_height);
    let land_mask = generate_land_mass(&mut rng, bounds, &cfg.terrain);

    // Step 2: Build hex map and assign terrain to ALL land tiles (no provinces yet).
    // Only coords inside the offset rectangle exist as tiles; the world's left
    // and right edges are vertical (within half a hex) instead of diagonal.
    let mut hex_map = HexMap::new(map_width, map_height);
    for coord in bounds.iter_coords() {
        hex_map.set_tile(coord, super::tile::Tile::new(TerrainType::Sea));
    }
    // Sort land tiles for deterministic RNG consumption (HashSet order is arbitrary)
    let mut land_tiles_sorted: Vec<HexCoord> = land_mask.iter().copied().collect();
    land_tiles_sorted.sort_by_key(|c| (c.q, c.r));
    for &coord in &land_tiles_sorted {
        let (terrain, resource) =
            random_land_terrain_with_resource(&mut rng, &cfg.terrain, coord, map_height);
        if let Some(res) = resource {
            hex_map.set_tile(coord, super::tile::Tile::with_resource(terrain, res));
        } else {
            hex_map.set_tile(coord, super::tile::Tile::new(terrain));
        }
    }

    // Step 2b: Cluster terrain for spatial coherence (forests→patches, mountains→chains, etc.)
    let mix = &cfg.terrain;
    cluster_terrain(
        &mut hex_map,
        &land_mask,
        &mut rng,
        TerrainType::Forest,
        mix.forest_cluster,
    );
    cluster_terrain(
        &mut hex_map,
        &land_mask,
        &mut rng,
        TerrainType::Hills,
        mix.hills_cluster,
    );
    cluster_terrain(
        &mut hex_map,
        &land_mask,
        &mut rng,
        TerrainType::Mountain,
        mix.mountain_cluster,
    );
    cluster_terrain(
        &mut hex_map,
        &land_mask,
        &mut rng,
        TerrainType::Desert,
        mix.desert_cluster,
    );
    cluster_terrain(
        &mut hex_map,
        &land_mask,
        &mut rng,
        TerrainType::Swamp,
        mix.swamp_cluster,
    );

    // Step 3: Cluster food terrain and enforce minimum food (before nations/provinces)
    cluster_food_terrain(&mut hex_map, &land_mask, &mut rng, 15);
    enforce_minimum_food_tiles(&mut hex_map, &land_mask, &mut rng, 20);

    // Step 3b: Add rivers after terrain is stable but before nations/provinces exist.
    let ocean_sea_tiles = ocean_sea_tiles(&hex_map);
    generate_rivers(&mut hex_map, &land_mask, &ocean_sea_tiles, &mut rng, &cfg.terrain);

    // Step 4: Place nation centers (terrain-aware — prefers food-rich, coastal tiles)
    let total_nations = num_gp + num_mn;
    let nation_centers = place_nation_centers(&mut rng, &land_mask, total_nations, &hex_map);

    // Step 5: Assign land tiles to nations (weighted Voronoi + normalization)
    let mut nation_tiles = assign_tiles_to_nations(&land_mask, &nation_centers, num_gp);
    normalize_territory_sizes(&mut nation_tiles, &nation_centers, total_nations, num_gp);

    // Step 6: Subdivide each nation's territory into provinces
    let mut province_id_counter: u32 = 0;
    let mut all_province_data: Vec<ProvinceData> = Vec::new();
    let mut nation_province_map: Vec<Vec<u32>> = Vec::with_capacity(total_nations);

    #[allow(clippy::needless_range_loop)]
    for nation_idx in 0..total_nations {
        let num_provinces = if nation_idx < num_gp {
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
    let deposit_coords: Vec<HexCoord> = bounds.iter_coords().collect();
    for coord in deposit_coords {
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

    // Step 11: Build Province structs
    let mut provinces: Vec<Province> = Vec::new();

    for pdata in &all_province_data {
        let nation_idx = pdata.nation_idx;
        let nation_name = if nation_idx < num_gp {
            great_power_name(nation_idx)
        } else {
            minor_nation_name(nation_idx - num_gp)
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

        let garrison = if nation_idx < num_gp { 4 } else { 3 };

        // Capital province is first in nation_province_map (set by select_country_capitals)
        let province_indices_for_nation = &nation_province_map[nation_idx];
        let is_capital_province = province_indices_for_nation
            .first()
            .is_some_and(|&first| first == pdata.id);

        let name = if is_capital_province {
            Province::capital_city_name(&nation_name)
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

    for (nation_idx, pids) in nation_province_map.iter().enumerate().take(total_nations) {
        let nation_id = NationId(nation_idx as u32);
        let name = if nation_idx < num_gp {
            great_power_name(nation_idx)
        } else {
            minor_nation_name(nation_idx - num_gp)
        };

        let province_ids: Vec<ProvinceId> = pids.iter().map(|&pid| ProvinceId(pid)).collect();

        let capital_province = province_ids[0];

        let setup = NationSetup {
            nation_id,
            name,
            province_ids,
            capital_province,
        };

        if nation_idx < num_gp {
            great_power_nations.push(setup);
        } else {
            minor_nations_out.push(setup);
        }
    }

    // Compute sea zones from the finished hex map
    let mut sea_zones = super::sea_zones::compute_sea_zones(&hex_map);
    // Assign coastal provinces to each sea zone and compute ocean_coastal per province
    super::sea_zones::assign_coastal_provinces(&mut sea_zones, &provinces, &hex_map);
    for province in &mut provinces {
        province.ocean_coastal = sea_zones
            .iter()
            .any(|z| !z.is_lake && z.coastal_provinces.contains(&province.id));
    }

    GeneratedMap {
        hex_map,
        provinces,
        great_power_nations,
        minor_nations: minor_nations_out,
        sea_zones,
    }
}

/// Validate that a generated map satisfies game invariants.
///
/// Checks:
/// - Province count matches `num_great_powers * 8 + num_minor_nations * 4`
/// - Each Great Power has exactly 8 provinces
/// - Each Minor Nation has exactly 4 provinces
/// - Every province has at least 1 tile
/// - Every province capital tile exists in the map
pub fn validate_map(map: &GeneratedMap) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    let expected = map.great_power_nations.len() * PROVINCES_PER_GREAT_POWER
        + map.minor_nations.len() * PROVINCES_PER_MINOR_NATION;
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

/// Linear-falloff multiplier in [0, 100] based on a coord's distance to the
/// nearest map edge. 100 once we're beyond `falloff_radius` cells, dropping
/// linearly to 0 at the `hard_margin`. This is the mechanism that produces an
/// organic, irregular coastline along the world's perimeter instead of a
/// rectangular sea band: tiles closer to the edge are progressively less
/// likely to be reached by continent growth.
fn edge_falloff_factor(
    coord: HexCoord,
    bounds: MapBounds,
    hard_margin: i32,
    falloff_radius: i32,
) -> i32 {
    let qoff = MapBounds::qoff(coord);
    let dist = qoff
        .min(bounds.width - 1 - qoff)
        .min(coord.r)
        .min(bounds.height - 1 - coord.r);
    if dist <= hard_margin {
        0
    } else if dist >= falloff_radius {
        100
    } else {
        let span = (falloff_radius - hard_margin).max(1);
        ((dist - hard_margin) * 100) / span
    }
}

fn ocean_sea_tiles(hex_map: &HexMap) -> HashSet<HexCoord> {
    let bounds = hex_map.bounds();
    let mut ocean = HashSet::new();
    let mut frontier = Vec::new();
    for coord in bounds.iter_coords() {
        if !bounds.is_edge_ring(coord, DEFAULT_SEA_HARD_MARGIN) {
            continue;
        }
        if hex_map
            .get_tile(coord)
            .is_some_and(|tile| tile.terrain() == TerrainType::Sea)
            && ocean.insert(coord)
        {
            frontier.push(coord);
        }
    }

    while let Some(coord) = frontier.pop() {
        for neighbor in coord.neighbors() {
            if ocean.contains(&neighbor) {
                continue;
            }
            if hex_map
                .get_tile(neighbor)
                .is_some_and(|tile| tile.terrain() == TerrainType::Sea)
            {
                ocean.insert(neighbor);
                frontier.push(neighbor);
            }
        }
    }

    ocean
}

fn river_tier(terrain: TerrainType) -> Option<u8> {
    match terrain {
        TerrainType::Mountain => Some(0),
        TerrainType::Hills => Some(1),
        TerrainType::Grassland => Some(2),
        _ => None,
    }
}

fn river_neighbor_allowed(current: TerrainType, next: TerrainType) -> bool {
    let Some(current_tier) = river_tier(current) else {
        return false;
    };
    let Some(next_tier) = river_tier(next) else {
        return false;
    };
    next_tier == current_tier || next_tier == current_tier + 1
}

fn find_river_path(
    source: HexCoord,
    hex_map: &HexMap,
    ocean_sea_tiles: &HashSet<HexCoord>,
    used_river_tiles: &HashSet<HexCoord>,
    rng: &mut Rng,
) -> Option<Vec<HexCoord>> {
    fn dfs_river_path(
        current: HexCoord,
        hex_map: &HexMap,
        ocean_sea_tiles: &HashSet<HexCoord>,
        used_river_tiles: &HashSet<HexCoord>,
        path: &mut Vec<HexCoord>,
        visited: &mut HashSet<HexCoord>,
        rng: &mut Rng,
    ) -> bool {
        let Some(current_tile) = hex_map.get_tile(current) else {
            return false;
        };
        if current_tile.terrain() == TerrainType::Grassland
            && current
                .neighbors()
                .iter()
                .any(|neighbor| ocean_sea_tiles.contains(neighbor))
        {
            return true;
        }

        let mut neighbors = current.neighbors();
        rng.shuffle(&mut neighbors);
        neighbors.sort_by_key(|coord| {
            let coast_bias = coord
                .neighbors()
                .iter()
                .filter(|neighbor| ocean_sea_tiles.contains(neighbor))
                .count();
            std::cmp::Reverse(coast_bias)
        });

        for neighbor in neighbors {
            if visited.contains(&neighbor) || used_river_tiles.contains(&neighbor) {
                continue;
            }
            if used_river_tiles.iter().any(|used| used.distance(neighbor) == 1) {
                continue;
            }
            let Some(next_tile) = hex_map.get_tile(neighbor) else {
                continue;
            };
            if !river_neighbor_allowed(current_tile.terrain(), next_tile.terrain()) {
                continue;
            }
            if path.iter().any(|coord| *coord != current && coord.distance(neighbor) == 1) {
                continue;
            }

            visited.insert(neighbor);
            path.push(neighbor);
            if dfs_river_path(
                neighbor,
                hex_map,
                ocean_sea_tiles,
                used_river_tiles,
                path,
                visited,
                rng,
            ) {
                return true;
            }
            path.pop();
            visited.remove(&neighbor);
        }

        false
    }

    let touches_used_river =
        |coord: HexCoord| used_river_tiles.iter().any(|used| used.distance(coord) == 1);
    let source_tile = hex_map.get_tile(source)?;
    if source_tile.terrain() != TerrainType::Mountain
        || used_river_tiles.contains(&source)
        || touches_used_river(source)
    {
        return None;
    }
    let mut path = vec![source];
    let mut visited: HashSet<HexCoord> = HashSet::from([source]);
    dfs_river_path(
        source,
        hex_map,
        ocean_sea_tiles,
        used_river_tiles,
        &mut path,
        &mut visited,
        rng,
    )
    .then_some(path)
}

fn generate_rivers(
    hex_map: &mut HexMap,
    land_mask: &HashSet<HexCoord>,
    ocean_sea_tiles: &HashSet<HexCoord>,
    rng: &mut Rng,
    mix: &TerrainMix,
) {
    if mix.river_source_percent <= 0 {
        return;
    }

    let mut candidate_sources: Vec<HexCoord> = land_mask
        .iter()
        .copied()
        .filter(|coord| {
            hex_map
                .get_tile(*coord)
                .is_some_and(|tile| tile.terrain() == TerrainType::Mountain)
        })
        .collect();
    candidate_sources.sort_by_key(|coord| (coord.q, coord.r));

    let mut eligibility_rng = Rng::from_seed(
        candidate_sources.len() as u64
            ^ hex_map.width() as u64
            ^ ((hex_map.height() as u64) << 32)
            ^ 0xA6E8_0F29_3D4B_C571,
    );
    let eligible_sources: Vec<HexCoord> = candidate_sources
        .iter()
        .copied()
        .filter(|&coord| {
            find_river_path(
                coord,
                hex_map,
                ocean_sea_tiles,
                &HashSet::new(),
                &mut eligibility_rng,
            )
            .is_some()
        })
        .collect();

    if eligible_sources.is_empty() {
        return;
    }

    let target_rivers =
        ((eligible_sources.len() * mix.river_source_percent as usize) + 50) / 100;
    if target_rivers == 0 {
        return;
    }

    let mut shuffled_sources = eligible_sources.clone();
    rng.shuffle(&mut shuffled_sources);

    let mut used_river_tiles: HashSet<HexCoord> = HashSet::new();
    let mut built = 0usize;
    for source in shuffled_sources {
        if built >= target_rivers {
            break;
        }
        let Some(path) = find_river_path(source, hex_map, ocean_sea_tiles, &used_river_tiles, rng)
        else {
            continue;
        };
        for coord in &path {
            used_river_tiles.insert(*coord);
            if let Some(tile) = hex_map.get_tile_mut(*coord) {
                tile.set_river(true);
            }
        }
        built += 1;
    }
}

/// Generate a land mask: a set of hex coordinates that are land.
/// Uses continent seeds that grow outward to create landmasses.
///
/// Edge handling: continents grow with a soft probability falloff as they
/// approach the rectangle's perimeter. Tiles in the outermost
/// `mix.sea_hard_margin` ring are never land; tiles within
/// `mix.sea_falloff_radius` have growth chance scaled by a linear factor.
/// Continents thus taper into ocean naturally, producing organic coastlines
/// around the world's edge instead of a clean rectangular cut.
fn generate_land_mass(rng: &mut Rng, bounds: MapBounds, mix: &TerrainMix) -> HashSet<HexCoord> {
    let hard_margin = mix.sea_hard_margin.max(0);
    let falloff_radius = mix.sea_falloff_radius.max(hard_margin + 1);
    let mut land = HashSet::new();

    // Place 5-8 continent seeds spread across the map. Seeds work in screen-
    // column space (qoff) so they're evenly distributed left-to-right
    // regardless of the diagonal-vs-vertical world shape.
    let num_continents = rng.range(5, 8) as usize;
    let map_width = bounds.width;
    let map_height = bounds.height;
    let mut seeds: Vec<HexCoord> = Vec::new();

    // Divide the map into vertical bands (in screen-column space) for spread.
    // Seeds are placed at least `falloff_radius + 1` cells from any edge so
    // continents have room to grow before the falloff kicks in.
    let band_margin = falloff_radius + 1;
    for i in 0..num_continents {
        let region_width = map_width / num_continents as i32;
        let qoff_low = (region_width * i as i32 + band_margin).min(map_width - band_margin - 1);
        let qoff_high = (qoff_low + region_width.max(4) - 2).min(map_width - band_margin - 1);
        let qoff = if qoff_high > qoff_low {
            rng.range(qoff_low, qoff_high)
        } else {
            qoff_low
        };
        let r_low = band_margin;
        let r_high = (map_height - band_margin - 1).max(r_low + 1);
        let r = rng.range(r_low, r_high);
        let q = qoff - r.div_euclid(2);
        let seed = HexCoord::new(q, r);
        seeds.push(seed);
    }

    // Grow each continent from its seed
    let land_amount = mix.land_amount.clamp(0.1, 4.0);
    for seed in &seeds {
        // Target size: ~200-500 tiles per continent, scaled by land_amount.
        let base_target = rng.range(200, 500) as f32;
        let target_size = (base_target * land_amount).max(20.0) as usize;
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
                // Hard floor: never grow into the outermost ring (guarantees
                // sea tiles exist on the very edge, so navy can sail past).
                if !bounds.contains_with_margin(*neighbor, hard_margin) {
                    continue;
                }
                // Distance-from-seed component (existing organic shaping).
                let dist = seed.distance(*neighbor);
                let dist_prob = if dist < 5 {
                    85
                } else if dist < 10 {
                    65
                } else if dist < 15 {
                    45
                } else {
                    25
                };
                // Edge-falloff multiplier so continents thin out organically
                // toward the world's perimeter.
                let edge_pct = edge_falloff_factor(*neighbor, bounds, hard_margin, falloff_radius);
                let prob = dist_prob * edge_pct / 100;

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
    num_great_powers: usize,
) -> HashMap<HexCoord, usize> {
    let mut assignments = HashMap::new();

    for &coord in land {
        let mut best_nation = 0;
        let mut best_dist = f64::MAX;

        for (idx, &center) in centers.iter().enumerate() {
            let raw_dist = coord.distance(center) as f64;
            // Minor nations have inflated distance → claim less territory
            let effective_dist = if idx >= num_great_powers {
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
    num_great_powers: usize,
) {
    let total_land = assignments.len();
    // Target: GP gets 2 shares, MN gets 1 share
    let total_shares = num_great_powers * 2 + (total_nations - num_great_powers);
    let share_size = if total_shares == 0 {
        0.0
    } else {
        total_land as f64 / total_shares as f64
    };

    let target_size = |nation_idx: usize| -> usize {
        if nation_idx < num_great_powers {
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
    tile.has_river()
        || matches!(
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
    for coord in hex_map.bounds().iter_coords() {
        if let Some(tile) = hex_map.get_tile(coord)
            && tile.terrain() == target_terrain
        {
            source_tiles.push(coord);
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
    for coord in hex_map.bounds().iter_coords() {
        if let Some(tile) = hex_map.get_tile(coord)
            && is_food_terrain(tile)
        {
            food_tiles.push((coord, tile.terrain(), tile.resource_deposit()));
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

/// Per-food-slot yield (grain, fruit, meat) a tile would produce inside the
/// capital harvest area, assuming the level-1 development head-start applied
/// at game init. Matches the rules used by the live turn processor:
///   * Grain / Fruit / Livestock at level 1 yield 2 in their respective slot.
///   * Bare Grassland tiles passively yield 1 Grain (Card #483).
///   * Everything else contributes nothing.
fn level1_food_yield(coord: HexCoord, hex_map: &super::hex_map::HexMap) -> (u32, u32, u32) {
    let Some(tile) = hex_map.get_tile(coord) else {
        return (0, 0, 0);
    };
    let mut grain = 0;
    let mut fruit = 0;
    let mut meat = 0;
    match tile.resource_deposit() {
        Some(ResourceType::Grain) => grain += 2,
        Some(ResourceType::Fruit) => fruit += 2,
        Some(ResourceType::Livestock) => meat += 2,
        None if tile.terrain() == TerrainType::Grassland => grain += 1,
        _ => {}
    }
    if tile.has_river() {
        meat += 1;
    }
    (grain, fruit, meat)
}

/// Score a tile as a potential capital location.
///
/// Picks the hex that supports the largest population. Workers eat balanced
/// composite meals (grain/fruit/meat in fixed proportions per
/// `worker_food_demand`), so the metric is the number of workers actually
/// feedable from the (grain, fruit, meat) supply at level-1 development
/// across capital + 6 neighbours — not raw total food.
///
/// Coastal capitals get the automatic port-fish yield (1 per adjacent ocean
/// hex, capped at 3); fish substitutes for livestock in the meat slot.
/// Coastal access and a non-barren landing site are small tiebreakers
/// between hexes with equal worker capacity.
fn score_capital_candidate(coord: HexCoord, hex_map: &super::hex_map::HexMap) -> u32 {
    let Some(capital_tile) = hex_map.get_tile(coord) else {
        return 0;
    };
    // A capital cannot stand on water or peaks.
    if matches!(
        capital_tile.terrain(),
        TerrainType::Sea | TerrainType::Mountain
    ) {
        return 0;
    }

    // Sum per-slot yields across the 7-tile capital harvest area at level 1.
    let (mut grain, mut fruit, mut meat) = level1_food_yield(coord, hex_map);
    let neighbors = coord.neighbors();
    for n in &neighbors {
        let (g, f, m) = level1_food_yield(*n, hex_map);
        grain += g;
        fruit += f;
        meat += m;
    }

    // Coastal capitals also fish — 1 Fish per adjacent ocean hex, capped at 3.
    // Fish folds into the meat slot (livestock-first consumption, then fish).
    let ocean_neighbors = neighbors
        .iter()
        .filter(|n| {
            hex_map
                .get_tile(**n)
                .map(|t| t.terrain() == TerrainType::Sea)
                .unwrap_or(false)
        })
        .count() as u32;
    let fish = ocean_neighbors.min(3);
    meat += fish;

    // Primary signal: workers supportable by the balanced food supply.
    // Tiebreakers: coastal access (port path), and a small penalty for
    // siting the capital on otherwise barren tiles (Desert/Tundra/Swamp)
    // with no resource of their own.
    let workers = crate::economy::labor::max_workers_supportable(grain, fruit, meat);
    let mut score = workers * 100;
    if ocean_neighbors > 0 {
        score += 10;
    }
    if matches!(
        capital_tile.terrain(),
        TerrainType::Desert | TerrainType::Tundra | TerrainType::Swamp
    ) && capital_tile.resource_deposit().is_none()
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
    let total_nations = nation_province_map.len();
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

/// Pick a random land terrain type and optional resource using the configured mix.
///
/// Tundra weight is biased by latitude — at full `pole_tundra_strength`, tiles
/// in the top and bottom rows get up to 5× the base tundra weight, while the
/// equator stays at the base weight. Resource rolls are conditional on the
/// chosen terrain and follow the historical surface-resource distribution.
fn random_land_terrain_with_resource(
    rng: &mut Rng,
    mix: &TerrainMix,
    coord: HexCoord,
    map_height: i32,
) -> (TerrainType, Option<ResourceType>) {
    // pole_strength ∈ [0, 1]: 0 at the equator (mid row), 1 at the top/bottom
    // edge of the world. Squared so the boost concentrates near the very edge
    // rather than fading linearly into the middle latitudes.
    let pole_strength = if map_height > 1 {
        let mid = (map_height as f32 - 1.0) * 0.5;
        let dist_from_mid = (coord.r as f32 - mid).abs();
        (dist_from_mid / mid).clamp(0.0, 1.0).powi(2)
    } else {
        0.0
    };
    let tundra_boost = 1.0 + mix.pole_tundra_strength.max(0.0) * 4.0 * pole_strength;
    let weights = [
        (TerrainType::Grassland, mix.grassland.max(0.0)),
        (TerrainType::Forest, mix.forest.max(0.0)),
        (TerrainType::Hills, mix.hills.max(0.0)),
        (TerrainType::Mountain, mix.mountain.max(0.0)),
        (TerrainType::Desert, mix.desert.max(0.0)),
        (TerrainType::Swamp, mix.swamp.max(0.0)),
        (TerrainType::Tundra, mix.tundra.max(0.0) * tundra_boost),
    ];
    let total: f32 = weights.iter().map(|(_, w)| *w).sum();
    let terrain = if total <= 0.0 {
        TerrainType::Grassland
    } else {
        let mut roll = (rng.next_u32() as f32 / u32::MAX as f32) * total;
        let mut chosen = TerrainType::Grassland;
        for (terrain, w) in &weights {
            if roll < *w {
                chosen = *terrain;
                break;
            }
            roll -= *w;
        }
        chosen
    };
    let resource = roll_surface_resource(rng, terrain);
    (terrain, resource)
}

/// Roll for a surface resource conditional on the terrain. Mineral deposits
/// for Mountain/Hills/Desert/Swamp/Tundra are placed in a separate prospecting
/// pass and so are not produced here.
fn roll_surface_resource(rng: &mut Rng, terrain: TerrainType) -> Option<ResourceType> {
    let roll = rng.range(0, 99);
    match terrain {
        TerrainType::Grassland => match roll {
            0..=54 => None,
            55..=70 => Some(ResourceType::Grain),
            71..=77 => Some(ResourceType::Fruit),
            78..=86 => Some(ResourceType::Cotton),
            87..=93 => Some(ResourceType::Livestock),
            _ => Some(ResourceType::Horses),
        },
        TerrainType::Forest => {
            if roll < 50 {
                Some(ResourceType::Timber)
            } else {
                None
            }
        }
        TerrainType::Hills => {
            if roll < 38 {
                Some(ResourceType::Wool)
            } else {
                None
            }
        }
        _ => None,
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
        assert_eq!(result.great_power_nations.len(), DEFAULT_NUM_GREAT_POWERS);
        assert_eq!(result.minor_nations.len(), DEFAULT_NUM_MINOR_NATIONS);
    }

    #[test]
    fn map_has_correct_number_of_provinces() {
        let result = generate_map("test");
        let expected = DEFAULT_NUM_GREAT_POWERS * PROVINCES_PER_GREAT_POWER
            + DEFAULT_NUM_MINOR_NATIONS * PROVINCES_PER_MINOR_NATION;
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
    fn outer_sea_ring_is_traversable() {
        // The outermost SEA_HARD_MARGIN ring of the offset rectangle must always be sea,
        // guaranteeing fleets can sail around the world's perimeter regardless of
        // where continents end up. The soft falloff just inside this ring may
        // contain land, by design — the rectangular look is broken up at the cost
        // of a shoreline that wiggles in and out of the falloff zone.
        for key in ["sea_ring_a", "sea_ring_b", "sea_ring_c", "sea_ring_d"] {
            let result = generate_map(key);
            let bounds = result.hex_map.bounds();
            for coord in bounds.iter_coords() {
                if !bounds.is_edge_ring(coord, DEFAULT_SEA_HARD_MARGIN) {
                    continue;
                }
                let tile = result.hex_map.get_tile(coord).unwrap_or_else(|| {
                    panic!("missing tile at edge-ring coord {coord} for key {key}")
                });
                assert_eq!(
                    tile.terrain(),
                    TerrainType::Sea,
                    "edge-ring coord {coord} (key {key}) is {:?}, expected Sea",
                    tile.terrain()
                );
            }
        }
    }

    #[test]
    fn pole_tundra_strength_concentrates_tundra_at_edges() {
        // With strong pole bias, tundra should be much more common in the top/
        // bottom 20% of rows than in the middle 20%.
        let mut cfg = MapGenConfig::default();
        cfg.terrain.pole_tundra_strength = 1.0;
        cfg.terrain.tundra = 5.0; // high enough that we get a meaningful sample
        let map = generate_map_with_config("pole_test", &cfg);
        let h = map.hex_map.height();
        let pole_threshold = h / 5;
        let mid_lo = h * 2 / 5;
        let mid_hi = h * 3 / 5;
        let mut pole_tundra = 0;
        let mut pole_total = 0;
        let mut mid_tundra = 0;
        let mut mid_total = 0;
        for (coord, tile) in map.hex_map.all_tiles() {
            if tile.terrain() == TerrainType::Sea {
                continue;
            }
            if coord.r < pole_threshold || coord.r >= h - pole_threshold {
                pole_total += 1;
                if tile.terrain() == TerrainType::Tundra {
                    pole_tundra += 1;
                }
            } else if coord.r >= mid_lo && coord.r < mid_hi {
                mid_total += 1;
                if tile.terrain() == TerrainType::Tundra {
                    mid_tundra += 1;
                }
            }
        }
        // Need at least some sample to compare meaningfully.
        assert!(pole_total > 30 && mid_total > 30);
        let pole_share = pole_tundra as f32 / pole_total as f32;
        let mid_share = mid_tundra as f32 / mid_total as f32;
        // Squared pole_strength + 5× max boost should produce a clear gradient.
        assert!(
            pole_share > mid_share * 1.5,
            "pole tundra share {pole_share} should exceed mid share {mid_share} by ≥1.5×"
        );
    }

    #[test]
    fn zero_pole_strength_does_not_concentrate_tundra() {
        // With strength = 0, tundra should be roughly uniform across rows.
        let mut cfg = MapGenConfig::default();
        cfg.terrain.pole_tundra_strength = 0.0;
        cfg.terrain.tundra = 10.0;
        let map = generate_map_with_config("uniform_test", &cfg);
        let h = map.hex_map.height();
        let pole_threshold = h / 5;
        let mut pole_tundra = 0;
        let mut pole_total = 0;
        let mut mid_tundra = 0;
        let mut mid_total = 0;
        for (coord, tile) in map.hex_map.all_tiles() {
            if tile.terrain() == TerrainType::Sea {
                continue;
            }
            if coord.r < pole_threshold || coord.r >= h - pole_threshold {
                pole_total += 1;
                if tile.terrain() == TerrainType::Tundra {
                    pole_tundra += 1;
                }
            } else {
                mid_total += 1;
                if tile.terrain() == TerrainType::Tundra {
                    mid_tundra += 1;
                }
            }
        }
        let pole_share = pole_tundra as f32 / pole_total.max(1) as f32;
        let mid_share = mid_tundra as f32 / mid_total.max(1) as f32;
        // No bias: shares should be within 2× of each other (random sample noise).
        assert!(
            pole_share < mid_share * 2.0 + 0.05,
            "with strength=0, pole {pole_share} and mid {mid_share} should be similar"
        );
    }

    #[test]
    fn map_bounds_form_offset_rectangle() {
        // Every row of the generated map should contain exactly map_width tiles
        // (the offset rectangle), not the parallelogram of the old axial layout.
        let result = generate_map("offset_rect_test");
        let bounds = result.hex_map.bounds();
        for r in 0..bounds.height {
            let count = (0..bounds.width)
                .filter(|qoff| {
                    let q = qoff - r.div_euclid(2);
                    result.hex_map.get_tile(HexCoord::new(q, r)).is_some()
                })
                .count();
            assert_eq!(
                count as i32, bounds.width,
                "row r={r} should have {} tiles, has {count}",
                bounds.width
            );
        }
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
    fn zero_river_percent_generates_no_rivers() {
        let mut cfg = MapGenConfig::default();
        cfg.terrain.river_source_percent = 0;
        let result = generate_map_with_config("no_rivers", &cfg);
        assert_eq!(
            result
                .hex_map
                .all_tiles()
                .filter(|(_, tile)| tile.has_river())
                .count(),
            0
        );
    }

    #[test]
    fn generated_rivers_form_unbranched_mountain_to_ocean_paths() {
        let mut cfg = MapGenConfig::default();
        cfg.terrain.grassland = 60.0;
        cfg.terrain.hills = 18.0;
        cfg.terrain.mountain = 16.0;
        cfg.terrain.forest = 3.0;
        cfg.terrain.desert = 1.0;
        cfg.terrain.swamp = 1.0;
        cfg.terrain.tundra = 1.0;
        cfg.terrain.river_source_percent = 100;
        let result = generate_map_with_config("river_paths", &cfg);
        let ocean = ocean_sea_tiles(&result.hex_map);
        let river_tiles: HashSet<HexCoord> = result
            .hex_map
            .all_tiles()
            .filter(|(_, tile)| tile.has_river())
            .map(|(coord, _)| coord)
            .collect();
        assert!(
            !river_tiles.is_empty(),
            "expected at least one river on a river-heavy map"
        );

        let mut remaining = river_tiles.clone();
        while let Some(&start) = remaining.iter().next() {
            let mut stack = vec![start];
            let mut component = Vec::new();
            remaining.remove(&start);
            while let Some(coord) = stack.pop() {
                component.push(coord);
                for neighbor in coord.neighbors() {
                    if remaining.remove(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }

            assert!(component.len() >= 3, "river component should span multiple hexes");

            let mut endpoints = Vec::new();
            for &coord in &component {
                let tile = result.hex_map.get_tile(coord).unwrap();
                assert!(
                    matches!(
                        tile.terrain(),
                        TerrainType::Mountain | TerrainType::Hills | TerrainType::Grassland
                    ),
                    "river tile {coord} has invalid terrain {:?}",
                    tile.terrain()
                );

                let degree = coord
                    .neighbors()
                    .iter()
                    .filter(|neighbor| component.contains(neighbor))
                    .count();
                assert!(degree <= 2, "river tile {coord} branched with degree {degree}");
                if degree <= 1 {
                    endpoints.push(coord);
                }

                for neighbor in coord.neighbors() {
                    if !component.contains(&neighbor) {
                        continue;
                    }
                    let neighbor_tile = result.hex_map.get_tile(neighbor).unwrap();
                    let a = river_tier(tile.terrain()).unwrap();
                    let b = river_tier(neighbor_tile.terrain()).unwrap();
                    assert!(
                        a.abs_diff(b) <= 1,
                        "river edge {coord} -> {neighbor} jumps tiers {:?} -> {:?}",
                        tile.terrain(),
                        neighbor_tile.terrain()
                    );
                }
            }

            assert_eq!(
                endpoints.len(),
                2,
                "river component should have exactly two endpoints"
            );
            assert!(
                endpoints.iter().any(|coord| {
                    result
                        .hex_map
                        .get_tile(*coord)
                        .is_some_and(|tile| tile.terrain() == TerrainType::Mountain)
                }),
                "river component should start in mountains"
            );
            assert!(
                endpoints.iter().any(|coord| {
                    result.hex_map.get_tile(*coord).is_some_and(|tile| {
                        tile.terrain() == TerrainType::Grassland
                            && coord.neighbors().iter().any(|neighbor| ocean.contains(neighbor))
                    })
                }),
                "river component should end on a coastal grassland hex"
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
