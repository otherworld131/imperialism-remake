//! Pointy-top hex math and mesh construction.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Outer radius of a hex tile in world units.
pub const HEX_SIZE: f32 = 24.0;
pub const SQRT_3: f32 = 1.732_050_8;

/// Axial (q, r) → world position. World y grows upward, map r grows
/// downward, hence the negation.
pub fn hex_to_world(q: i32, r: i32) -> Vec2 {
    let x = HEX_SIZE * (SQRT_3 * q as f32 + SQRT_3 / 2.0 * r as f32);
    let y = HEX_SIZE * (3.0 / 2.0 * r as f32);
    Vec2::new(x, -y)
}

/// World position → axial (q, r), the inverse of [`hex_to_world`].
pub fn world_to_hex(pos: Vec2) -> (i32, i32) {
    let x = pos.x / HEX_SIZE;
    let y = -pos.y / HEX_SIZE;
    let qf = SQRT_3 / 3.0 * x - y / 3.0;
    let rf = 2.0 / 3.0 * y;
    axial_round(qf, rf)
}

/// Horizontal world-space period of a map that wraps after `map_width`
/// columns.
pub fn world_width_px(map_width: i32) -> f32 {
    SQRT_3 * HEX_SIZE * map_width as f32
}

fn axial_round(qf: f32, rf: f32) -> (i32, i32) {
    let sf = -qf - rf;
    let mut q = qf.round();
    let mut r = rf.round();
    let s = sf.round();
    let dq = (q - qf).abs();
    let dr = (r - rf).abs();
    let ds = (s - sf).abs();
    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    (q as i32, r as i32)
}

fn pointy_hex_angle(index: usize) -> f32 {
    (60.0 * index as f32 + 30.0).to_radians()
}

/// Append one filled hex (center fan, 7 vertices / 18 indices) to a mesh
/// under construction. This is the merged-mesh building block: one mesh per
/// terrain/nation group instead of one entity per tile.
pub fn append_hex(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    center: Vec2,
    radius: f32,
) {
    let base = positions.len() as u32;
    positions.push([center.x, center.y, 0.0]);
    normals.push([0.0, 0.0, 1.0]);
    uvs.push([0.5, 0.5]);

    for i in 0..6 {
        let angle = pointy_hex_angle(i);
        let x = radius * angle.cos();
        let y = radius * angle.sin();
        positions.push([center.x + x, center.y + y, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([(x / radius + 1.0) * 0.5, (y / radius + 1.0) * 0.5]);
    }

    for i in 1..=6u32 {
        let next = if i == 6 { 1 } else { i + 1 };
        indices.extend_from_slice(&[base, base + i, base + next]);
    }
}

/// One mesh covering every hex center in `centers`.
pub fn merged_hex_mesh(centers: &[Vec2], radius: f32) -> Mesh {
    let mut positions = Vec::with_capacity(centers.len() * 7);
    let mut normals = Vec::with_capacity(centers.len() * 7);
    let mut uvs = Vec::with_capacity(centers.len() * 7);
    let mut indices = Vec::with_capacity(centers.len() * 18);
    for &center in centers {
        append_hex(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            center,
            radius,
        );
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// Hexagonal ring outline (hover/selection markers).
pub fn pointy_hex_ring_mesh(outer_radius: f32, inner_radius: f32) -> Mesh {
    let mut positions = Vec::with_capacity(12);
    let mut normals = Vec::with_capacity(12);
    let mut uvs = Vec::with_capacity(12);

    for radius in [outer_radius, inner_radius] {
        for i in 0..6 {
            let angle = pointy_hex_angle(i);
            let x = radius * angle.cos();
            let y = radius * angle.sin();
            positions.push([x, y, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
            uvs.push([
                (x / outer_radius + 1.0) * 0.5,
                (y / outer_radius + 1.0) * 0.5,
            ]);
        }
    }

    let mut indices = Vec::with_capacity(36);
    for i in 0..6u32 {
        let next = if i == 5 { 0 } else { i + 1 };
        let inner_a = i + 6;
        let inner_b = next + 6;
        indices.extend_from_slice(&[i, inner_a, next]);
        indices.extend_from_slice(&[next, inner_a, inner_b]);
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_hex_inverts_hex_to_world() {
        for q in -30..30 {
            for r in -10..40 {
                assert_eq!(world_to_hex(hex_to_world(q, r)), (q, r));
            }
        }
    }
}
