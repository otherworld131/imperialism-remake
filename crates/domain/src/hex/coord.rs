/// Axial hex coordinate using the "pointy-top" convention.
///
/// Uses cube constraint: q + r + s = 0 (s is derived, not stored).
/// See: https://www.redblobgames.com/grids/hexagons/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

/// The six directions in a pointy-top hex grid.
pub const HEX_DIRECTIONS: [HexCoord; 6] = [
    HexCoord { q: 1, r: 0 },
    HexCoord { q: 1, r: -1 },
    HexCoord { q: 0, r: -1 },
    HexCoord { q: -1, r: 0 },
    HexCoord { q: -1, r: 1 },
    HexCoord { q: 0, r: 1 },
];

impl HexCoord {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// The implicit third cube coordinate: s = -q - r.
    pub const fn s(self) -> i32 {
        -self.q - self.r
    }

    /// Hex distance (Manhattan distance in cube coordinates / 2).
    pub fn distance(self, other: Self) -> i32 {
        let dq = (self.q - other.q).abs();
        let dr = (self.r - other.r).abs();
        let ds = (self.s() - other.s()).abs();
        (dq + dr + ds) / 2
    }

    /// The 6 adjacent hexes.
    pub fn neighbors(self) -> [HexCoord; 6] {
        HEX_DIRECTIONS.map(|d| self + d)
    }

    /// All hexes at exactly `radius` distance (a ring).
    /// Returns empty vec for radius 0 (use the coord itself).
    pub fn ring(self, radius: i32) -> Vec<HexCoord> {
        if radius <= 0 {
            return vec![];
        }

        let mut results = Vec::with_capacity(6 * radius as usize);
        // Start at the "bottom-left" of the ring
        let mut current = Self::new(self.q - radius, self.r + radius);

        for dir in &HEX_DIRECTIONS {
            for _ in 0..radius {
                results.push(current);
                current = current + *dir;
            }
        }
        results
    }

    /// All hexes within `radius` distance (a filled circle), excluding self.
    pub fn range(self, radius: i32) -> Vec<HexCoord> {
        let mut results = Vec::new();
        for q in -radius..=radius {
            let r_min = (-radius).max(-q - radius);
            let r_max = radius.min(-q + radius);
            for r in r_min..=r_max {
                let coord = Self::new(self.q + q, self.r + r);
                if coord != self {
                    results.push(coord);
                }
            }
        }
        results
    }

    /// Line from self to other using hex linear interpolation.
    pub fn line_to(self, other: Self) -> Vec<HexCoord> {
        let n = self.distance(other);
        if n == 0 {
            return vec![self];
        }

        let mut results = Vec::with_capacity(n as usize + 1);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            results.push(hex_lerp(self, other, t));
        }
        results
    }

    /// Convert axial hex coordinate to pixel position (pointy-top).
    /// Returns (x, y) with the given hex size (outer radius).
    pub fn to_pixel(self, size: f64) -> (f64, f64) {
        let x = size * (3.0_f64.sqrt() * self.q as f64 + 3.0_f64.sqrt() / 2.0 * self.r as f64);
        let y = size * (3.0 / 2.0 * self.r as f64);
        (x, y)
    }

    /// Convert pixel position back to the nearest hex coordinate (pointy-top).
    pub fn from_pixel(x: f64, y: f64, size: f64) -> Self {
        let q = (3.0_f64.sqrt() / 3.0 * x - 1.0 / 3.0 * y) / size;
        let r = (2.0 / 3.0 * y) / size;
        hex_round(q, r)
    }
}

impl std::ops::Add for HexCoord {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.q + rhs.q, self.r + rhs.r)
    }
}

impl std::ops::Sub for HexCoord {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.q - rhs.q, self.r - rhs.r)
    }
}

impl std::ops::Neg for HexCoord {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.q, -self.r)
    }
}

impl std::fmt::Display for HexCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.q, self.r)
    }
}

/// Round fractional axial coordinates to the nearest hex.
fn hex_round(q: f64, r: f64) -> HexCoord {
    let s = -q - r;

    let mut rq = q.round();
    let mut rr = r.round();
    let rs = s.round();

    let q_diff = (rq - q).abs();
    let r_diff = (rr - r).abs();
    let s_diff = (rs - s).abs();

    if q_diff > r_diff && q_diff > s_diff {
        rq = -rr - rs;
    } else if r_diff > s_diff {
        rr = -rq - rs;
    }
    // else: s gets corrected, but we don't store s

    HexCoord::new(rq as i32, rr as i32)
}

/// Linear interpolation between two hex coordinates.
fn hex_lerp(a: HexCoord, b: HexCoord, t: f64) -> HexCoord {
    let q = a.q as f64 + (b.q - a.q) as f64 * t;
    let r = a.r as f64 + (b.r - a.r) as f64 * t;
    // Nudge by epsilon to avoid ambiguous rounding on edges
    hex_round(q + 1e-6, r + 1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction & cube constraint ──────────────────────────

    #[test]
    fn cube_constraint_holds() {
        let c = HexCoord::new(3, -7);
        assert_eq!(c.q + c.r + c.s(), 0);
    }

    #[test]
    fn origin() {
        let o = HexCoord::new(0, 0);
        assert_eq!(o.q, 0);
        assert_eq!(o.r, 0);
        assert_eq!(o.s(), 0);
    }

    #[test]
    fn cube_constraint_negative() {
        let c = HexCoord::new(-5, 2);
        assert_eq!(c.q + c.r + c.s(), 0);
    }

    // ── Equality & hashing ──────────────────────────────────────

    #[test]
    fn equality() {
        assert_eq!(HexCoord::new(1, 2), HexCoord::new(1, 2));
        assert_ne!(HexCoord::new(1, 2), HexCoord::new(2, 1));
    }

    #[test]
    fn hash_consistency() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(HexCoord::new(1, 2));
        set.insert(HexCoord::new(1, 2)); // duplicate
        assert_eq!(set.len(), 1);
    }

    // ── Arithmetic ──────────────────────────────────────────────

    #[test]
    fn add() {
        let a = HexCoord::new(1, 2);
        let b = HexCoord::new(3, -1);
        assert_eq!(a + b, HexCoord::new(4, 1));
    }

    #[test]
    fn sub() {
        let a = HexCoord::new(4, 1);
        let b = HexCoord::new(3, -1);
        assert_eq!(a - b, HexCoord::new(1, 2));
    }

    #[test]
    fn neg() {
        assert_eq!(-HexCoord::new(3, -2), HexCoord::new(-3, 2));
    }

    // ── Distance ────────────────────────────────────────────────

    #[test]
    fn distance_to_self_is_zero() {
        let c = HexCoord::new(5, -3);
        assert_eq!(c.distance(c), 0);
    }

    #[test]
    fn distance_to_neighbor_is_one() {
        let c = HexCoord::new(0, 0);
        for n in c.neighbors() {
            assert_eq!(c.distance(n), 1);
        }
    }

    #[test]
    fn distance_symmetry() {
        let a = HexCoord::new(2, -3);
        let b = HexCoord::new(-1, 4);
        assert_eq!(a.distance(b), b.distance(a));
    }

    #[test]
    fn distance_known_values() {
        let o = HexCoord::new(0, 0);
        assert_eq!(o.distance(HexCoord::new(3, 0)), 3);
        assert_eq!(o.distance(HexCoord::new(0, 3)), 3);
        assert_eq!(o.distance(HexCoord::new(-3, 3)), 3);
        assert_eq!(o.distance(HexCoord::new(2, -2)), 2);
        assert_eq!(o.distance(HexCoord::new(3, -1)), 3);
    }

    // ── Neighbors ───────────────────────────────────────────────

    #[test]
    fn neighbors_count() {
        let c = HexCoord::new(0, 0);
        assert_eq!(c.neighbors().len(), 6);
    }

    #[test]
    fn neighbors_all_distance_one() {
        let c = HexCoord::new(4, -2);
        for n in c.neighbors() {
            assert_eq!(c.distance(n), 1);
        }
    }

    #[test]
    fn neighbors_unique() {
        let c = HexCoord::new(0, 0);
        let ns = c.neighbors();
        let set: std::collections::HashSet<_> = ns.iter().collect();
        assert_eq!(set.len(), 6);
    }

    // ── Ring ────────────────────────────────────────────────────

    #[test]
    fn ring_zero_is_empty() {
        assert!(HexCoord::new(0, 0).ring(0).is_empty());
    }

    #[test]
    fn ring_one_equals_neighbors() {
        let c = HexCoord::new(0, 0);
        let ring: std::collections::HashSet<_> = c.ring(1).into_iter().collect();
        let neigh: std::collections::HashSet<_> = c.neighbors().into_iter().collect();
        assert_eq!(ring, neigh);
    }

    #[test]
    fn ring_size() {
        let c = HexCoord::new(0, 0);
        assert_eq!(c.ring(1).len(), 6);
        assert_eq!(c.ring(2).len(), 12);
        assert_eq!(c.ring(3).len(), 18);
        assert_eq!(c.ring(5).len(), 30);
    }

    #[test]
    fn ring_all_at_correct_distance() {
        let c = HexCoord::new(2, -3);
        for r in 1..=5 {
            for hex in c.ring(r) {
                assert_eq!(
                    c.distance(hex),
                    r,
                    "hex {hex} should be at distance {r} from {c}"
                );
            }
        }
    }

    // ── Range ───────────────────────────────────────────────────

    #[test]
    fn range_size() {
        let c = HexCoord::new(0, 0);
        // range excludes self, so 3*r*(r+1) for radius r
        assert_eq!(c.range(1).len(), 6);
        assert_eq!(c.range(2).len(), 18);
        assert_eq!(c.range(3).len(), 36);
    }

    #[test]
    fn range_does_not_contain_self() {
        let c = HexCoord::new(3, -1);
        assert!(!c.range(5).contains(&c));
    }

    #[test]
    fn range_all_within_distance() {
        let c = HexCoord::new(0, 0);
        for hex in c.range(4) {
            assert!(c.distance(hex) <= 4);
            assert!(c.distance(hex) >= 1);
        }
    }

    // ── Line drawing ────────────────────────────────────────────

    #[test]
    fn line_to_self() {
        let c = HexCoord::new(2, 3);
        assert_eq!(c.line_to(c), vec![c]);
    }

    #[test]
    fn line_length_equals_distance_plus_one() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(3, -1);
        let line = a.line_to(b);
        assert_eq!(line.len(), a.distance(b) as usize + 1);
    }

    #[test]
    fn line_starts_and_ends_correctly() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(4, -2);
        let line = a.line_to(b);
        assert_eq!(*line.first().unwrap(), a);
        assert_eq!(*line.last().unwrap(), b);
    }

    #[test]
    fn line_consecutive_hexes_are_adjacent() {
        let a = HexCoord::new(-2, 3);
        let b = HexCoord::new(3, -1);
        let line = a.line_to(b);
        for pair in line.windows(2) {
            assert_eq!(
                pair[0].distance(pair[1]),
                1,
                "consecutive hexes {} and {} should be adjacent",
                pair[0],
                pair[1]
            );
        }
    }

    // ── Pixel conversion ────────────────────────────────────────

    #[test]
    fn origin_to_pixel_is_zero() {
        let (x, y) = HexCoord::new(0, 0).to_pixel(10.0);
        assert!((x).abs() < 1e-9);
        assert!((y).abs() < 1e-9);
    }

    #[test]
    fn pixel_roundtrip() {
        let size = 32.0;
        for q in -5..=5 {
            for r in -5..=5 {
                let original = HexCoord::new(q, r);
                let (px, py) = original.to_pixel(size);
                let recovered = HexCoord::from_pixel(px, py, size);
                assert_eq!(
                    original, recovered,
                    "roundtrip failed for ({q}, {r}) → pixel ({px}, {py}) → {recovered}"
                );
            }
        }
    }

    #[test]
    fn from_pixel_snaps_to_nearest() {
        let size = 10.0;
        let target = HexCoord::new(1, 0);
        let (cx, cy) = target.to_pixel(size);
        // Slightly off-center should still snap to same hex
        let recovered = HexCoord::from_pixel(cx + 1.0, cy + 1.0, size);
        assert_eq!(recovered, target);
    }

    // ── Display ─────────────────────────────────────────────────

    #[test]
    fn display_format() {
        assert_eq!(format!("{}", HexCoord::new(3, -7)), "(3, -7)");
    }

    // ── Large coordinate edge cases ─────────────────────────────

    #[test]
    fn distance_large_coordinates() {
        let origin = HexCoord::new(0, 0);
        let far = HexCoord::new(100, -50);
        let dist = origin.distance(far);
        // With cube coords: dq=100, dr=50, ds=|0-(-100+50)|=|50|=50
        // distance = max(100, 50, 50) = 100
        assert_eq!(dist, 100);
    }

    #[test]
    fn ring_radius_10_has_exactly_60_hexes() {
        let c = HexCoord::new(0, 0);
        let ring = c.ring(10);
        assert_eq!(
            ring.len(),
            60,
            "Ring of radius 10 should have 6*10 = 60 hexes"
        );
        // All should be at distance 10
        for hex in &ring {
            assert_eq!(c.distance(*hex), 10);
        }
    }

    #[test]
    fn line_between_adjacent_hexes_has_length_2() {
        let a = HexCoord::new(0, 0);
        let b = HexCoord::new(1, 0); // adjacent neighbor
        assert_eq!(a.distance(b), 1);
        let line = a.line_to(b);
        assert_eq!(
            line.len(),
            2,
            "Line between two adjacent hexes should have 2 hexes (start and end)"
        );
        assert_eq!(line[0], a);
        assert_eq!(line[1], b);
    }

    #[test]
    fn ring_negative_radius_is_empty() {
        let c = HexCoord::new(0, 0);
        assert!(c.ring(-1).is_empty());
        assert!(c.ring(-100).is_empty());
    }

    #[test]
    fn distance_large_negative_coordinates() {
        let a = HexCoord::new(-50, 25);
        let b = HexCoord::new(50, -25);
        // symmetry check
        assert_eq!(a.distance(b), b.distance(a));
        assert_eq!(a.distance(b), 100);
    }
}
