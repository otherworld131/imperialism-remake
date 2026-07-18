//! 2D mesh builder: polyline → triangle-strip triangulation (miter joins),
//! dashes, arrows, circles, and raw segment quads. Everything appends into
//! one growing buffer so a whole layer becomes a single `Mesh2d`.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Accumulates triangles; `build()` produces one merged mesh.
#[derive(Default)]
pub struct MeshBuilder2d {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    /// Explicit UVs, filled only by the `add_textured_*` methods. A builder
    /// must be either fully textured or fully untextured — `build()` falls
    /// back to zero UVs when the buffer doesn't cover every vertex.
    uvs: Vec<[f32; 2]>,
}

/// Miter length clamp, in multiples of the half-width. Sharp spikes at acute
/// joins fall back to a bevel-ish clamped miter.
const MITER_LIMIT: f32 = 4.0;

impl MeshBuilder2d {
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    fn push_quad(&mut self, a: Vec2, b: Vec2, c: Vec2, d: Vec2) {
        // Quad as two triangles: a-b-c, a-c-d.
        let base = self.positions.len() as u32;
        for p in [a, b, c, d] {
            self.positions.push([p.x, p.y, 0.0]);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn push_triangle(&mut self, a: Vec2, b: Vec2, c: Vec2) {
        let base = self.positions.len() as u32;
        for p in [a, b, c] {
            self.positions.push([p.x, p.y, 0.0]);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// Constant-width stroke along `pts` with clamped miter joins. `closed`
    /// connects the last point back to the first.
    pub fn add_polyline_strip(&mut self, pts: &[Vec2], width: f32, closed: bool) {
        // Drop consecutive duplicates — zero-length segments break normals.
        let mut clean: Vec<Vec2> = Vec::with_capacity(pts.len());
        for &p in pts {
            if clean.last().is_none_or(|last| last.distance(p) > 1e-4) {
                clean.push(p);
            }
        }
        if closed && clean.len() > 1 && clean[0].distance(clean[clean.len() - 1]) <= 1e-4 {
            clean.pop();
        }
        let n = clean.len();
        if n < 2 {
            return;
        }
        let half = width / 2.0;
        let seg_dir = |i: usize| -> Vec2 { (clean[(i + 1) % n] - clean[i]).normalize_or_zero() };

        // Per-vertex offset vector (miter).
        let offset_at = |i: usize| -> Vec2 {
            let has_prev = closed || i > 0;
            let has_next = closed || i + 1 < n;
            let d_prev = if has_prev {
                seg_dir((i + n - 1) % n)
            } else {
                Vec2::ZERO
            };
            let d_next = if has_next { seg_dir(i) } else { Vec2::ZERO };
            let (d0, d1) = match (has_prev, has_next) {
                (true, true) => (d_prev, d_next),
                (true, false) => (d_prev, d_prev),
                (false, true) => (d_next, d_next),
                (false, false) => (Vec2::X, Vec2::X),
            };
            let n0 = Vec2::new(-d0.y, d0.x);
            let n1 = Vec2::new(-d1.y, d1.x);
            let m = (n0 + n1).normalize_or_zero();
            if m == Vec2::ZERO {
                // 180° turn: fall back to the incoming normal.
                return n0 * half;
            }
            let denom = m.dot(n1).max(1.0 / MITER_LIMIT);
            m * (half / denom)
        };

        let base = self.positions.len() as u32;
        for (i, p) in clean.iter().enumerate() {
            let off = offset_at(i);
            self.positions.push([p.x + off.x, p.y + off.y, 0.0]);
            self.positions.push([p.x - off.x, p.y - off.y, 0.0]);
        }
        let seg_count = if closed { n } else { n - 1 };
        for i in 0..seg_count as u32 {
            let j = (i + 1) % n as u32;
            let (a, b) = (base + i * 2, base + i * 2 + 1);
            let (c, d) = (base + j * 2, base + j * 2 + 1);
            self.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    /// One quad per straight segment — for hex grids / rail ties where joins
    /// don't matter.
    pub fn add_segment(&mut self, a: Vec2, b: Vec2, width: f32) {
        let dir = (b - a).normalize_or_zero();
        if dir == Vec2::ZERO {
            return;
        }
        let n = Vec2::new(-dir.y, dir.x) * (width / 2.0);
        self.push_quad(a + n, b + n, b - n, a - n);
    }

    /// Textured quad from `a` to `b` with arc-length UVs: U runs 0 →
    /// `distance / u_period` (the texture tiles every `u_period` world units
    /// along the segment), V spans 0 → 1 across the width. Requires a
    /// repeat-sampled texture and must not be mixed with untextured geometry
    /// in the same builder (see `uvs`).
    pub fn add_textured_segment(&mut self, a: Vec2, b: Vec2, width: f32, u_period: f32) {
        let len = a.distance(b);
        if len <= 1e-4 || u_period <= 0.0 {
            return;
        }
        let dir = (b - a) / len;
        let n = Vec2::new(-dir.y, dir.x) * (width / 2.0);
        let u1 = len / u_period;
        // push_quad order: a+n, b+n, b-n, a-n.
        self.push_quad(a + n, b + n, b - n, a - n);
        self.uvs
            .extend_from_slice(&[[0.0, 0.0], [u1, 0.0], [u1, 1.0], [0.0, 1.0]]);
    }

    /// Textured axis-aligned quad mapping the full texture once (U and V both
    /// 0 → 1) onto a square of `size` centered at `center`. Used for the rail
    /// node pads.
    pub fn add_textured_quad(&mut self, center: Vec2, size: f32) {
        let h = size / 2.0;
        self.push_quad(
            center + Vec2::new(-h, h),
            center + Vec2::new(h, h),
            center + Vec2::new(h, -h),
            center + Vec2::new(-h, -h),
        );
        self.uvs
            .extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    }

    /// Dashed straight line from `a` to `b`.
    pub fn add_dashed_line(&mut self, a: Vec2, b: Vec2, width: f32, dash: f32, gap: f32) {
        let total = a.distance(b);
        if total <= 1e-4 || dash <= 0.0 {
            return;
        }
        let dir = (b - a) / total;
        let mut t = 0.0;
        while t < total {
            let end = (t + dash).min(total);
            self.add_segment(a + dir * t, a + dir * end, width);
            t = end + gap;
        }
    }

    /// Filled arrowhead with the tip at `tip` pointing along `dir` (unit),
    /// matching the React shape: back corners at `tip - len*rot(±spread)`.
    pub fn add_arrowhead(&mut self, tip: Vec2, dir: Vec2, len: f32, spread: f32) {
        let angle = dir.y.atan2(dir.x);
        let left = tip - Vec2::from_angle(angle - spread) * len;
        let right = tip - Vec2::from_angle(angle + spread) * len;
        self.push_triangle(tip, left, right);
    }

    pub fn add_circle(&mut self, center: Vec2, radius: f32, segments: usize) {
        let segments = segments.max(3);
        let base = self.positions.len() as u32;
        self.positions.push([center.x, center.y, 0.0]);
        for i in 0..segments {
            let a = i as f32 / segments as f32 * std::f32::consts::TAU;
            self.positions.push([
                center.x + radius * a.cos(),
                center.y + radius * a.sin(),
                0.0,
            ]);
        }
        for i in 0..segments as u32 {
            let next = if i + 1 == segments as u32 { 0 } else { i + 1 };
            self.indices
                .extend_from_slice(&[base, base + 1 + i, base + 1 + next]);
        }
    }

    pub fn add_ring(&mut self, center: Vec2, inner: f32, outer: f32, segments: usize) {
        let segments = segments.max(3);
        let base = self.positions.len() as u32;
        for i in 0..segments {
            let a = i as f32 / segments as f32 * std::f32::consts::TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let po = center + dir * outer;
            let pi = center + dir * inner;
            self.positions.push([po.x, po.y, 0.0]);
            self.positions.push([pi.x, pi.y, 0.0]);
        }
        for i in 0..segments as u32 {
            let j = if i + 1 == segments as u32 { 0 } else { i + 1 };
            let (a, b) = (base + i * 2, base + i * 2 + 1);
            let (c, d) = (base + j * 2, base + j * 2 + 1);
            self.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    /// Ribbon between matched point pairs (`inner[i]` ↔ `outer[i]`). Used by
    /// the organic fill-correction strips where both rails share parameters.
    pub fn add_ribbon(&mut self, inner: &[Vec2], outer: &[Vec2]) {
        let n = inner.len().min(outer.len());
        if n < 2 {
            return;
        }
        let base = self.positions.len() as u32;
        for i in 0..n {
            self.positions.push([inner[i].x, inner[i].y, 0.0]);
            self.positions.push([outer[i].x, outer[i].y, 0.0]);
        }
        for i in 0..(n as u32 - 1) {
            let (a, b) = (base + i * 2, base + i * 2 + 1);
            let (c, d) = (base + i * 2 + 2, base + i * 2 + 3);
            self.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    pub fn build(self) -> Mesh {
        let count = self.positions.len();
        let uvs = if self.uvs.len() == count {
            self.uvs
        } else {
            vec![[0.0, 0.0]; count]
        };
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(builder: &MeshBuilder2d) -> &[[f32; 3]] {
        &builder.positions
    }

    #[test]
    fn straight_strip_offsets_by_half_width() {
        let mut b = MeshBuilder2d::default();
        b.add_polyline_strip(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)], 2.0, false);
        let pos = positions(&b);
        assert_eq!(pos.len(), 4);
        // Left rail at +1, right rail at -1 (normal of +x is +y).
        assert_eq!(pos[0][1], 1.0);
        assert_eq!(pos[1][1], -1.0);
        assert_eq!(pos[2][0], 10.0);
        assert_eq!(b.indices.len(), 6);
    }

    #[test]
    fn right_angle_miter_meets_at_45_degrees() {
        let mut b = MeshBuilder2d::default();
        b.add_polyline_strip(
            &[
                Vec2::new(0.0, 0.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 10.0),
            ],
            2.0,
            false,
        );
        let pos = positions(&b);
        assert_eq!(pos.len(), 6);
        // Interior vertex (index 1) miter offset: half / cos(45°) = √2 along
        // the (-1, 1)/√2 direction → corner points at (9, 1) and (11, -1).
        let corner_a = pos[2];
        let corner_b = pos[3];
        assert!((corner_a[0] - 9.0).abs() < 1e-4 && (corner_a[1] - 1.0).abs() < 1e-4);
        assert!((corner_b[0] - 11.0).abs() < 1e-4 && (corner_b[1] - (-1.0)).abs() < 1e-4);
    }

    #[test]
    fn closed_strip_has_segment_per_vertex() {
        let mut b = MeshBuilder2d::default();
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        b.add_polyline_strip(&square, 1.0, true);
        assert_eq!(positions(&b).len(), 8);
        assert_eq!(b.indices.len(), 4 * 6);
    }

    #[test]
    fn dashes_cover_expected_count() {
        let mut b = MeshBuilder2d::default();
        b.add_dashed_line(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, 2.0, 2.0);
        // Dashes at [0,2], [4,6], [8,10] → 3 quads → 12 vertices.
        assert_eq!(positions(&b).len(), 12);
    }

    #[test]
    fn textured_segment_uvs_span_arc_length() {
        let mut b = MeshBuilder2d::default();
        b.add_textured_segment(Vec2::ZERO, Vec2::new(48.0, 0.0), 12.0, 24.0);
        assert_eq!(b.positions.len(), 4);
        assert_eq!(b.uvs.len(), 4);
        // U tiles twice over 48 units at period 24; V spans 0 → 1.
        assert_eq!(b.uvs[0], [0.0, 0.0]);
        assert_eq!(b.uvs[1], [2.0, 0.0]);
        assert_eq!(b.uvs[2], [2.0, 1.0]);
        assert_eq!(b.uvs[3], [0.0, 1.0]);
    }

    #[test]
    fn mixed_builder_falls_back_to_zero_uvs() {
        let mut b = MeshBuilder2d::default();
        b.add_textured_segment(Vec2::ZERO, Vec2::new(10.0, 0.0), 2.0, 10.0);
        b.add_segment(Vec2::ZERO, Vec2::new(5.0, 0.0), 1.0);
        let mesh = b.build();
        let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap();
        // 8 vertices but only 4 recorded UVs → all-zero fallback.
        assert_eq!(uvs.len(), 8);
    }

    #[test]
    fn ribbon_pairs_points() {
        let mut b = MeshBuilder2d::default();
        let inner = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(2.0, 0.0),
        ];
        let outer = [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 1.0),
        ];
        b.add_ribbon(&inner, &outer);
        assert_eq!(positions(&b).len(), 6);
        assert_eq!(b.indices.len(), 12);
    }
}
