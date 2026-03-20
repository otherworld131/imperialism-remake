use std::collections::HashMap;

use crate::hex::HexCoord;
use crate::types::*;

use super::tile::Tile;

/// The hex-based game map. Stores all tiles indexed by axial hex coordinates.
///
/// Not every coordinate within the bounding rectangle necessarily has a tile —
/// the map can have irregular coastlines, islands, etc.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct HexMap {
    #[serde(
        serialize_with = "serialize_hex_tiles",
        deserialize_with = "deserialize_hex_tiles"
    )]
    tiles: HashMap<HexCoord, Tile>,
    width: i32,
    height: i32,
}

/// Serialize HashMap<HexCoord, Tile> as a Vec of (HexCoord, Tile) pairs
/// because HexCoord cannot be used directly as a JSON object key.
fn serialize_hex_tiles<S>(tiles: &HashMap<HexCoord, Tile>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::Serialize;
    let entries: Vec<(&HexCoord, &Tile)> = tiles.iter().collect();
    entries.serialize(serializer)
}

/// Deserialize Vec of (HexCoord, Tile) pairs back into HashMap<HexCoord, Tile>.
fn deserialize_hex_tiles<'de, D>(deserializer: D) -> Result<HashMap<HexCoord, Tile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let entries: Vec<(HexCoord, Tile)> = Vec::deserialize(deserializer)?;
    Ok(entries.into_iter().collect())
}

impl HexMap {
    /// Create an empty hex map with the given logical dimensions.
    ///
    /// The width and height define the bounding rectangle in hex columns/rows.
    /// No tiles are placed — call `set_tile` to populate.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            tiles: HashMap::new(),
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
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        let tile = map.get_tile(coord).unwrap();
        assert_eq!(tile.terrain(), TerrainType::Farm);
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
        map.set_tile(coord, Tile::new(TerrainType::Farm));

        let tile = map.get_tile_mut(coord).unwrap();
        tile.is_capital = true;

        assert!(map.get_tile(coord).unwrap().is_capital);
    }

    #[test]
    fn set_tile_overwrites() {
        let mut map = HexMap::new(10, 10);
        let coord = HexCoord::new(2, 2);
        map.set_tile(coord, Tile::new(TerrainType::Farm));
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
            Tile::with_province(TerrainType::Farm, pid),
        );
        map.set_tile(
            HexCoord::new(1, 0),
            Tile::with_province(TerrainType::Farm, pid),
        );
        map.set_tile(
            HexCoord::new(2, 0),
            Tile::with_province(TerrainType::Farm, ProvinceId(2)),
        );
        map.set_tile(HexCoord::new(3, 0), Tile::new(TerrainType::Sea));

        let province_tiles = map.tiles_in_province(pid);
        assert_eq!(province_tiles.len(), 2);
    }

    #[test]
    fn adjacent_tiles_returns_only_existing() {
        let mut map = HexMap::new(10, 10);
        let center = HexCoord::new(5, 5);
        map.set_tile(center, Tile::new(TerrainType::Farm));

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
    fn tile_count_tracks_insertions() {
        let mut map = HexMap::new(10, 10);
        assert_eq!(map.tile_count(), 0);
        map.set_tile(HexCoord::new(0, 0), Tile::new(TerrainType::Farm));
        assert_eq!(map.tile_count(), 1);
        map.set_tile(HexCoord::new(1, 0), Tile::new(TerrainType::Farm));
        assert_eq!(map.tile_count(), 2);
    }
}
