use crate::hex::HexCoord;

/// The "offset rectangle" world-shape. Hexes form a screen-space rectangle
/// of width × height hex cells with edges that are as vertical as a
/// pointy-top hex grid allows (every other row offset by half a hex).
///
/// In axial coords with pointy-top, a hex (q, r) is inside iff:
///   0 <= r < height
///   0 <= q + r.div_euclid(2) < width
///
/// "Offset-q" `qoff = q + r.div_euclid(2)` is the screen column index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapBounds {
    pub width: i32,
    pub height: i32,
}

impl MapBounds {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn qoff(coord: HexCoord) -> i32 {
        coord.q + coord.r.div_euclid(2)
    }

    #[inline]
    pub const fn contains(&self, coord: HexCoord) -> bool {
        coord.r >= 0
            && coord.r < self.height
            && Self::qoff(coord) >= 0
            && Self::qoff(coord) < self.width
    }

    #[inline]
    pub const fn contains_with_margin(&self, coord: HexCoord, margin: i32) -> bool {
        coord.r >= margin
            && coord.r < self.height - margin
            && Self::qoff(coord) >= margin
            && Self::qoff(coord) < self.width - margin
    }

    /// True iff `coord` is inside the rectangle and within `margin` cells of any edge.
    /// Used to identify the forced-sea ring that frames the map.
    #[inline]
    pub const fn is_edge_ring(&self, coord: HexCoord, margin: i32) -> bool {
        self.contains(coord) && !self.contains_with_margin(coord, margin)
    }

    /// Iterate every coord inside the rectangle in deterministic (r ascending, qoff ascending) order.
    pub fn iter_coords(self) -> impl Iterator<Item = HexCoord> {
        (0..self.height).flat_map(move |r| {
            let shift = r.div_euclid(2);
            (0..self.width).map(move |qoff| HexCoord::new(qoff - shift, r))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_origin() {
        let b = MapBounds::new(10, 10);
        assert!(b.contains(HexCoord::new(0, 0)));
    }

    #[test]
    fn contains_offset_corners() {
        let b = MapBounds::new(10, 10);
        // Row 0: q ∈ [0, 10)
        assert!(b.contains(HexCoord::new(0, 0)));
        assert!(b.contains(HexCoord::new(9, 0)));
        assert!(!b.contains(HexCoord::new(-1, 0)));
        assert!(!b.contains(HexCoord::new(10, 0)));
        // Row 2: shift = 1, q ∈ [-1, 9)
        assert!(b.contains(HexCoord::new(-1, 2)));
        assert!(b.contains(HexCoord::new(8, 2)));
        assert!(!b.contains(HexCoord::new(-2, 2)));
        assert!(!b.contains(HexCoord::new(9, 2)));
        // Row 4: shift = 2, q ∈ [-2, 8)
        assert!(b.contains(HexCoord::new(-2, 4)));
        assert!(b.contains(HexCoord::new(7, 4)));
    }

    #[test]
    fn iter_coords_count_matches_area() {
        let b = MapBounds::new(7, 5);
        let coords: Vec<HexCoord> = b.iter_coords().collect();
        assert_eq!(coords.len(), 7 * 5);
        for c in coords {
            assert!(b.contains(c));
        }
    }

    #[test]
    fn iter_coords_screen_rectangle_in_pixel_space() {
        // Every row's pixel-x range is approximately [0, width * sqrt(3) * size).
        // No row's leftmost pixel-x deviates more than half a hex from 0.
        let b = MapBounds::new(20, 20);
        let size = 1.0;
        let sqrt3 = 3.0_f64.sqrt();
        let world_width = b.width as f64 * sqrt3 * size;
        for r in 0..b.height {
            let row_coords: Vec<HexCoord> =
                b.iter_coords().filter(|c| c.r == r).collect();
            let leftmost = row_coords.iter().map(|c| c.to_pixel(size).0).fold(f64::INFINITY, f64::min);
            let rightmost = row_coords.iter().map(|c| c.to_pixel(size).0).fold(f64::NEG_INFINITY, f64::max);
            // leftmost is either 0 (even rows) or 0.5*sqrt3 (odd rows)
            assert!(leftmost >= -1e-9 && leftmost < sqrt3 * size, "row {r}: leftmost {leftmost}");
            assert!(rightmost <= world_width + 1e-9, "row {r}: rightmost {rightmost} world_width {world_width}");
        }
    }

    #[test]
    fn margin_excludes_outer_ring() {
        let b = MapBounds::new(10, 10);
        assert!(!b.contains_with_margin(HexCoord::new(0, 0), 2));
        assert!(!b.contains_with_margin(HexCoord::new(1, 1), 2));
        assert!(b.contains_with_margin(HexCoord::new(2, 2), 2));
        // Edge ring tile is in bounds but inside the ring
        assert!(b.is_edge_ring(HexCoord::new(0, 0), 2));
        assert!(!b.is_edge_ring(HexCoord::new(2, 2), 2));
    }
}
