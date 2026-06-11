//! Nation / province label placement — a faithful Rust port of
//! `web/src/lib/nationLabels.ts`.
//!
//! Land tiles are grouped per visual group (or owner), each group is split
//! into BFS-connected components over the hex grid, and every component
//! large enough to warrant a label gets one. The label anchor is the member
//! tile whose pixel center is closest to the component centroid, so labels
//! never float over sea or a neighbour on concave territory.
//!
//! Pure placement math, no Bevy types. Pixel coordinates use the React
//! convention (y grows downward, axial pointy-top layout at `hex_size`);
//! the renderer converts at the boundary.

use std::collections::{HashMap, HashSet};

/// Hex size the React frontend uses by default.
pub const DEFAULT_HEX_SIZE: f64 = 18.0;
/// Minimum tiles a connected component needs to receive a label.
pub const DEFAULT_MIN_COMPONENT_SIZE: usize = 3;

/// Minimal tile shape required by [`compute_nation_labels`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelTile {
    /// Axial column.
    pub q: i32,
    /// Axial row.
    pub r: i32,
    /// Sea tiles never carry labels.
    pub is_sea: bool,
    /// Owning nation; empty = unowned (tile is skipped).
    pub owner: String,
    /// Visual group (incorporated-minor parent); empty = fall back to owner.
    pub visual_group: String,
    /// Whether the owner is in anarchy (recorded from the group's first tile,
    /// matching the TS original).
    pub is_anarchic: bool,
}

/// One label placement for a connected component of owned land tiles.
#[derive(Debug, Clone, PartialEq)]
pub struct NationLabel {
    /// Group name (visual group or owner).
    pub name: String,
    /// Anchor pixel x (React convention, see module docs).
    pub cx: f64,
    /// Anchor pixel y.
    pub cy: f64,
    /// Component size in tiles — the LOD/sizing input for rendering.
    pub size: usize,
    /// Whether the owning nation is anarchic.
    pub is_anarchic: bool,
}

fn hex_neighbors(q: i32, r: i32) -> [(i32, i32); 6] {
    [
        (q + 1, r),
        (q, r + 1),
        (q - 1, r + 1),
        (q - 1, r),
        (q, r - 1),
        (q + 1, r - 1),
    ]
}

/// Group land tiles into BFS-connected components per visual group (or
/// owner) and return a label per component of at least `min_size` tiles.
/// The React call site uses `min_size = `[`DEFAULT_MIN_COMPONENT_SIZE`] and
/// `hex_size = `[`DEFAULT_HEX_SIZE`].
#[must_use]
pub fn compute_nation_labels(
    tiles: &[LabelTile],
    min_size: usize,
    hex_size: f64,
) -> Vec<NationLabel> {
    let sqrt3 = 3.0_f64.sqrt();
    let hex_to_pixel = |q: i32, r: i32| -> (f64, f64) {
        (
            hex_size * (sqrt3 * f64::from(q) + sqrt3 / 2.0 * f64::from(r)),
            hex_size * (3.0 / 2.0 * f64::from(r)),
        )
    };

    struct Group {
        /// Insertion-ordered member coordinates (mirrors the JS `Set` order).
        order: Vec<(i32, i32)>,
        members: HashSet<(i32, i32)>,
        is_anarchic: bool,
    }

    let mut group_order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, Group> = HashMap::new();
    for tile in tiles {
        if tile.is_sea || tile.owner.is_empty() {
            continue;
        }
        let name: &str = if tile.visual_group.is_empty() {
            &tile.owner
        } else {
            &tile.visual_group
        };
        let entry = groups.entry(name).or_insert_with(|| {
            group_order.push(name);
            Group {
                order: Vec::new(),
                members: HashSet::new(),
                is_anarchic: tile.is_anarchic,
            }
        });
        if entry.members.insert((tile.q, tile.r)) {
            entry.order.push((tile.q, tile.r));
        }
    }

    let mut labels: Vec<NationLabel> = Vec::new();
    for name in &group_order {
        let Some(entry) = groups.get(*name) else {
            continue;
        };
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        for &start in &entry.order {
            if visited.contains(&start) {
                continue;
            }
            // BFS flood fill over same-group neighbours.
            let mut component: Vec<(i32, i32)> = Vec::new();
            let mut queue: Vec<(i32, i32)> = vec![start];
            let mut head = 0;
            visited.insert(start);
            while head < queue.len() {
                let (cq, cr) = queue[head];
                head += 1;
                component.push((cq, cr));
                for nb in hex_neighbors(cq, cr) {
                    if entry.members.contains(&nb) && visited.insert(nb) {
                        queue.push(nb);
                    }
                }
            }
            if component.len() < min_size {
                continue;
            }
            // Centroid, then anchor on the member pixel nearest to it.
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut pixels: Vec<(f64, f64)> = Vec::with_capacity(component.len());
            for &(cq, cr) in &component {
                let (px, py) = hex_to_pixel(cq, cr);
                pixels.push((px, py));
                sx += px;
                sy += py;
            }
            let count = component.len() as f64;
            let centroid_x = sx / count;
            let centroid_y = sy / count;
            let mut best = pixels[0];
            let mut best_dist = f64::INFINITY;
            for &(px, py) in &pixels {
                let dx = px - centroid_x;
                let dy = py - centroid_y;
                let d = dx * dx + dy * dy;
                if d < best_dist {
                    best_dist = d;
                    best = (px, py);
                }
            }
            labels.push(NationLabel {
                name: (*name).to_owned(),
                cx: best.0,
                cy: best.1,
                size: component.len(),
                is_anarchic: entry.is_anarchic,
            });
        }
    }
    labels
}

// ── Reference tests ────────────────────────────────────────────────────────
//
// Expected values were captured by executing the TypeScript original
// (web/src/lib/nationLabels.ts transpiled with esbuild --format=cjs, run
// under node v22) on the exact fixture below.
#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn tile(
        q: i32,
        r: i32,
        is_sea: bool,
        owner: &str,
        visual_group: &str,
        is_anarchic: bool,
    ) -> LabelTile {
        LabelTile {
            q,
            r,
            is_sea,
            owner: owner.to_owned(),
            visual_group: visual_group.to_owned(),
            is_anarchic,
        }
    }

    fn fixture() -> Vec<LabelTile> {
        vec![
            // Redland component 1: four connected tiles.
            tile(0, 0, false, "Redland", "", false),
            tile(1, 0, false, "Redland", "", false),
            tile(0, 1, false, "Redland", "", false),
            tile(1, 1, false, "Redland", "", false),
            // Redland component 2: three connected tiles, far away.
            tile(10, 0, false, "Redland", "", false),
            tile(11, 0, false, "Redland", "", false),
            tile(10, 1, false, "Redland", "", false),
            // Redland lone tile: filtered out at min_size 3.
            tile(5, 5, false, "Redland", "", false),
            // Bluemark with a visual_group override and anarchy on the
            // group's first tile.
            tile(0, 5, false, "Bluemark", "Bluemark Empire", true),
            tile(1, 5, false, "Bluemark", "Bluemark Empire", false),
            tile(0, 6, false, "Bluemark", "Bluemark Empire", false),
            // Sea tile and unowned tile: both skipped.
            tile(3, 3, true, "Redland", "", false),
            tile(7, 7, false, "", "", false),
        ]
    }

    fn assert_label(
        got: &NationLabel,
        name: &str,
        cx: f64,
        cy: f64,
        size: usize,
        is_anarchic: bool,
    ) {
        assert_eq!(got.name, name);
        assert!(
            (got.cx - cx).abs() < EPS && (got.cy - cy).abs() < EPS,
            "{name}: anchor ({}, {}) expected ({cx}, {cy})",
            got.cx,
            got.cy
        );
        assert_eq!(got.size, size, "{name}: size");
        assert_eq!(got.is_anarchic, is_anarchic, "{name}: is_anarchic");
    }

    #[test]
    fn component_detection_matches_ts_reference() {
        // TS: computeNationLabels(tiles) — minSize 3, hexSize 18.
        let labels = compute_nation_labels(&fixture(), 3, 18.0);
        assert_eq!(labels.len(), 3, "labels: {labels:?}");
        // Two disconnected Redland components, the lone tile filtered, the
        // sea/unowned tiles skipped, and Bluemark labelled under its visual
        // group with the first tile's anarchy flag.
        assert_label(&labels[0], "Redland", 31.17691453623979, 0.0, 4, false);
        assert_label(&labels[1], "Redland", 311.76914536239786, 0.0, 3, false);
        assert_label(
            &labels[2],
            "Bluemark Empire",
            109.11920087683924,
            135.0,
            3,
            true,
        );
    }

    #[test]
    fn placement_with_min_size_one_and_hex_size_24_matches_ts_reference() {
        // TS: computeNationLabels(tiles, 1, 24) — lone tile now labelled,
        // anchors scaled to the Bevy hex size.
        let labels = compute_nation_labels(&fixture(), 1, 24.0);
        assert_eq!(labels.len(), 4, "labels: {labels:?}");
        assert_label(&labels[0], "Redland", 41.569219381653056, 0.0, 4, false);
        assert_label(&labels[1], "Redland", 457.2614131981836, 0.0, 3, false);
        assert_label(&labels[2], "Redland", 311.76914536239786, 180.0, 1, false);
        assert_label(
            &labels[3],
            "Bluemark Empire",
            145.49226783578567,
            180.0,
            3,
            true,
        );
    }
}
