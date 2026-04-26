use std::collections::BTreeMap;

use crate::hex::HexCoord;
use crate::types::*;

use super::tile::Tile;

/// The hex-based game map. Stores all tiles indexed by axial hex coordinates.
///
/// Not every coordinate within the bounding rectangle necessarily has a tile —
/// the map can have irregular coastlines, islands, etc.
pub struct HexMap {
    tiles: BTreeMap<HexCoord, Tile>,
    width: i32,
    height: i32,
}

impl HexMap {
    /// Create an empty hex map with the given logical dimensions.
    ///
    /// The width and height define the bounding rectangle in hex columns/rows.
    /// No tiles are placed — call `set_tile` to populate.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            tiles: BTreeMap::new(),
            width,
            height,
        }
    }

    /// Get an immutable reference to the tile at the given coordinate.
    pub fn get_tile(&self, coord: HexCoord) -> Option<&Tile> {
        self.tiles.get(&coord)
    }

    /// Get a mutable reference to the tile at the given coordinate.
    pub fn get_tile_mut(&mut self, coord: HexCoord) -> Option<&mut Tile> {
        self.tiles.get_mut(&coord)
    }

    /// Place (or overwrite) a tile at the given coordinate.
    pub fn set_tile(&mut self, coord: HexCoord, tile: Tile) {
        self.tiles.insert(coord, tile);
    }

    /// Return all tiles belonging to a given province.
    pub fn tiles_in_province(&self, province_id: ProvinceId) -> Vec<(HexCoord, &Tile)> {
        self.tiles
            .iter()
            .filter(|(_, tile)| tile.province_id == Some(province_id))
            .map(|(&coord, tile)| (coord, tile))
            .collect()
    }

    /// Return tiles adjacent to the given coordinate that actually exist in the map.
    pub fn adjacent_tiles(&self, coord: HexCoord) -> Vec<(HexCoord, &Tile)> {
        coord
            .neighbors()
            .into_iter()
            .filter_map(|neighbor| self.tiles.get(&neighbor).map(|tile| (neighbor, tile)))
            .collect()
    }

    /// Get all tiles within `radius` hex distance of `center` that exist in the map.
    ///
    /// Uses `HexCoord::range` to compute candidate coordinates and filters to
    /// those present in the map. The center tile itself is **not** included
    /// (consistent with `HexCoord::range` which excludes self).
    pub fn tiles_in_range(&self, center: HexCoord, radius: i32) -> Vec<(HexCoord, &Tile)> {
        center
            .range(radius)
            .into_iter()
            .filter_map(|coord| self.get_tile(coord).map(|tile| (coord, tile)))
            .collect()
    }

    /// Iterate over all tiles in the map.
    pub fn all_tiles(&self) -> impl Iterator<Item = (HexCoord, &Tile)> {
        self.tiles.iter().map(|(&coord, tile)| (coord, tile))
    }

    /// The number of tiles currently in the map.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// The logical width of the map (number of columns).
    pub fn width(&self) -> i32 {
        self.width
    }

    /// The logical height of the map (number of rows).
    pub fn height(&self) -> i32 {
        self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TerrainType;

    #[test]
    fn new_map_is_empty() {
        let map = HexMap::new(60, 40);
        assert_eq!(map.tile_count(), 0);
        assert_eq!(map.width(), 60);
        assert_eq!(map.height(), 40);
    }

    #[test]
    fn set_and_get_tile() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(3, 5);
        map.set_tile(coord, Tile::new(TerrainType::Grassland));

        let tile = map.get_tile(coord).unwrap();
        assert_eq!(tile.terrain(), TerrainType::Grassland);
    }

    #[test]
    fn get_tile_returns_none_for_missing() {
        let map = HexMap::new(10, 10);
        assert!(map.get_tile(HexCoord::new(0, 0)).is_none());
    }

    #[test]
    fn get_tile_mut_can_modify() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(1, 1);
        map.set_tile(coord, Tile::new(TerrainType::Grassland));

        let tile = map.get_tile_mut(coord).unwrap();
        tile.is_capital = true;

        assert!(map.get_tile(coord).unwrap().is_capital);
    }

    #[test]
    fn set_tile_overwrites() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(2, 2);
        map.set_tile(coord, Tile::new(TerrainType::Grassland));
        map.set_tile(coord, Tile::new(TerrainType::Mountain));

        assert_eq!(
            map.get_tile(coord).unwrap().terrain(),
            TerrainType::Mountain
        );
        assert_eq!(map.tile_count(), 1);
    }

    #[test]
    fn tiles_in_province() {
        let mut map = HexMap::new(10, 10);
        let pid = ProvinceId(1);
        map.set_tile(
            HexCoord::new(0, 0),
            Tile::with_province(TerrainType::Grassland, pid),
        );
        map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Grassland, pid),
        );
        map.set_tile(
            HexCoord::new(2, 0),
            Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        map.set_tile(HexCoord::new(3, 0), Tile::new(TerrainType::Sea));

        let province_tiles = map.tiles_in_province(pid);
        assert_eq!(province_tiles.len(), 2);
    }

    #[test]
    fn adjacent_tiles_returns_only_existing() {
        let mut map = HexMap::new(10, 10);
        let center = HexCoord::new(5, 5);
        map.set_tile(center, Tile::new(TerrainType::Grassland));

        // Place tiles at only 3 of the 6 neighbors
        let neighbors = center.neighbors();
        map.set_tile(neighbors[0], Tile::new(TerrainType::Mountain));
        map.set_tile(neighbors[1], Tile::new(TerrainType::Sea));
        map.set_tile(neighbors[2], Tile::new(TerrainType::Desert));

        let adj = map.adjacent_tiles(center);
        assert_eq!(adj.len(), 3);
    }

    #[test]
    fn all_tiles_iterates_everything() {
        let mut map = HexMap::new(10, 10);
        for q in 0..5 {
            for r in 0..5 {
                map.set_tile(HexCoord::new(q, r), Tile::new(TerrainType::Sea));
            }
        }
        assert_eq!(map.all_tiles().count(), 25);
    }

    #[test]
    fn tiles_in_range_radius_1_returns_up_to_6() {
        let mut map = HexMap::new(20, 20);
        let center = HexCoord::new(5, 5);
        map.set_tile(center, Tile::new(TerrainType::Grassland));

        // Place all 6 neighbors
        for n in center.neighbors() {
            map.set_tile(n, Tile::new(TerrainType::Grassland));
        }

        let in_range = map.tiles_in_range(center, 1);
        // Range excludes self, so should return exactly 6 neighbors
        assert_eq!(in_range.len(), 6);
        // Center should not be in the result
        assert!(in_range.iter().all(|(coord, _)| *coord != center));
    }

    #[test]
    fn tiles_in_range_radius_0_returns_empty() {
        let mut map = HexMap::new(10, 10);
        let center = HexCoord::new(3, 3);
        map.set_tile(center, Tile::new(TerrainType::Grassland));
        for n in center.neighbors() {
            map.set_tile(n, Tile::new(TerrainType::Grassland));
        }

        // Radius 0: HexCoord::range(0) returns empty, so nothing is returned
        let in_range = map.tiles_in_range(center, 0);
        assert!(in_range.is_empty());
    }

    #[test]
    fn tiles_in_range_only_returns_existing_tiles() {
        let mut map = HexMap::new(20, 20);
        let center = HexCoord::new(10, 10);
        map.set_tile(center, Tile::new(TerrainType::Grassland));

        // Place only 3 of the 6 neighbors
        let neighbors = center.neighbors();
        map.set_tile(neighbors[0], Tile::new(TerrainType::Mountain));
        map.set_tile(neighbors[2], Tile::new(TerrainType::Sea));
        map.set_tile(neighbors[4], Tile::new(TerrainType::Desert));

        let in_range = map.tiles_in_range(center, 1);
        // Only 3 neighbors exist in the map
        assert_eq!(in_range.len(), 3);
    }

    #[test]
    fn tiles_in_range_radius_2_includes_ring_1_and_ring_2() {
        let mut map = HexMap::new(30, 30);
        let center = HexCoord::new(15, 15);
        map.set_tile(center, Tile::new(TerrainType::Grassland));

        // Fill all tiles within radius 2
        for coord in center.range(2) {
            map.set_tile(coord, Tile::new(TerrainType::Grassland));
        }

        let in_range = map.tiles_in_range(center, 2);
        // range(2) = 18 tiles (excludes self)
        assert_eq!(in_range.len(), 18);
        // All should be at distance <= 2 and >= 1 from center
        for (coord, _) in &in_range {
            let d = center.distance(*coord);
            assert!((1..=2).contains(&d));
        }
    }

    #[test]
    fn tile_count_tracks_insertions() {
        let mut map = HexMap::new(10, 10);
        assert_eq!(map.tile_count(), 0);
        map.set_tile(HexCoord::new(0, 0), Tile::new(TerrainType::Grassland));
        assert_eq!(map.tile_count(), 1);
        map.set_tile(HexCoord::new(1, 0), Tile::new(TerrainType::Grassland));
        assert_eq!(map.tile_count(), 2);
    }
}
