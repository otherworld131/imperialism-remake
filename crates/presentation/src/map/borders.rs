//! Map edge classification + organic border geometry — a port of the
//! `mapGeometry` / `classifiedEdges` memos in `web/src/components/HexMap.tsx`.
//!
//! Works in the React pixel convention (y grows downward, pointy-top axial
//! layout at the Bevy hex size); the rendering layer negates y at the mesh
//! boundary. All displacement runs through [`crate::map::organic`], the
//! bit-faithful port of the web noise/smoothing pipeline, so both frontends
//! agree on the math.

use std::collections::{HashMap, HashSet};

use crate::game::vm::MapTile;
use crate::map::organic::{
    self, AnchoredOpts, BORDER_OCTAVES, BORDER_SEED, BORDER_SLOPE_CLAMP, BORDER_SMOOTHING,
    COAST_AMPLITUDE_FRAC, COAST_SUBDIV, COUNTRY_BORDER_AMPLITUDE_FRAC, COUNTRY_BORDER_SUBDIV, Edge,
    PROVINCE_BORDER_AMPLITUDE_FRAC, PROVINCE_BORDER_SUBDIV, Point, border_frequency,
    ruggedness_frequency, ruggedness_multiplier, smooth_polyline_anchored, stitch_polylines,
};

/// A smoothed border polyline in React pixel space.
#[derive(Debug, Clone)]
pub struct Polyline {
    pub pts: Vec<Point>,
    pub closed: bool,
}

/// One classified hex edge with its smoothed curve, for the organic
/// fill-correction ribbons. `normal` is the canonical displacement normal
/// (shared by both sides of the edge); `outward_sign` is +1 when that normal
/// points away from `tile_idx` (toward the sea / the neighbor).
#[derive(Debug, Clone)]
pub struct FillStrip {
    pub curve: Vec<Point>,
    pub base_a: Point,
    pub normal: Point,
    pub outward_sign: f64,
    pub tile_idx: usize,
    pub neighbor_idx: Option<usize>,
}

/// Everything the layer renderer needs, computed once per map version.
#[derive(Debug, Default, Clone)]
pub struct MapBorders {
    pub coast_strokes: Vec<Polyline>,
    pub country_strokes: Vec<Polyline>,
    pub province_strokes: Vec<Polyline>,
    pub coast_strips: Vec<FillStrip>,
    pub country_strips: Vec<FillStrip>,
    /// Interior same-province hex edges (deduped) — the subtle hex grid.
    pub grid_segments: Vec<[Point; 2]>,
    /// Straight-edge fallback when organic borders are off: country borders
    /// plus owned coastline, in the country stroke style.
    pub straight_country_segments: Vec<[Point; 2]>,
    /// Straight-edge fallback: province borders.
    pub straight_province_segments: Vec<[Point; 2]>,
}

/// The 6 vertex offsets of a pointy-top hex, React order (60°·i − 30°).
pub fn hex_vertices(size: f64) -> [Point; 6] {
    let mut verts = [[0.0; 2]; 6];
    for (i, v) in verts.iter_mut().enumerate() {
        let angle = (60.0 * i as f64 - 30.0).to_radians();
        *v = [size * angle.cos(), size * angle.sin()];
    }
    verts
}

/// Neighbor order matching the React edge indexing (E, SE, SW, W, NW, NE):
/// edge `i` runs between vertex `i` and vertex `i + 1`.
pub fn hex_neighbors(q: i32, r: i32) -> [(i32, i32); 6] {
    [
        (q + 1, r),
        (q, r + 1),
        (q - 1, r + 1),
        (q - 1, r),
        (q, r - 1),
        (q + 1, r - 1),
    ]
}

/// React-convention pixel center (y down) at `hex_size`.
pub fn hex_to_pixel(q: i32, r: i32, hex_size: f64) -> Point {
    let sqrt3 = 3.0_f64.sqrt();
    [
        hex_size * (sqrt3 * f64::from(q) + sqrt3 / 2.0 * f64::from(r)),
        hex_size * (1.5 * f64::from(r)),
    ]
}

/// Wrap an axial coordinate into the primary map copy in offset-q space —
/// the world is an offset rectangle, so naive q-modulo would leave the row's
/// stored range. Mirrors `wrapNeighbor` in HexMap.tsx.
pub fn wrap_axial(q: i32, r: i32, map_width: i32) -> (i32, i32) {
    if map_width <= 0 {
        return (q, r);
    }
    let shift = (f64::from(r) / 2.0).floor() as i32;
    let qoff = q + shift;
    let wqoff = qoff.rem_euclid(map_width);
    (wqoff - shift, r)
}

fn political_key(k1: &str, k2: &str) -> String {
    if k1 < k2 {
        format!("{k1}|{k2}")
    } else {
        format!("{k2}|{k1}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeClass {
    Coast,
    Country,
    Province,
    Interior,
}

/// Classify every land-tile hex edge and produce smoothed border geometry.
pub fn classify(tiles: &[MapTile], hex_size: f64) -> MapBorders {
    if tiles.is_empty() {
        return MapBorders::default();
    }
    let verts = hex_vertices(hex_size);
    let map_width = tiles[0].map_width;
    let frequency = border_frequency(hex_size);
    let rugged_freq = ruggedness_frequency(hex_size);
    let coast_amp = COAST_AMPLITUDE_FRAC * hex_size;
    let country_amp = COUNTRY_BORDER_AMPLITUDE_FRAC * hex_size;
    let province_amp = PROVINCE_BORDER_AMPLITUDE_FRAC * hex_size;

    let index: HashMap<(i32, i32), usize> = tiles
        .iter()
        .enumerate()
        .map(|(i, t)| ((t.q, t.r), i))
        .collect();
    let neighbor_at = |nq: i32, nr: i32| -> Option<usize> {
        let (wq, wr) = wrap_axial(nq, nr, map_width);
        index.get(&(wq, wr)).copied()
    };

    let mut vertex_coord: HashMap<String, Point> = HashMap::new();
    let mut edge_normal: HashMap<String, Point> = HashMap::new();
    let mut edge_amp_mult: HashMap<String, f64> = HashMap::new();
    let mut seen_political: HashSet<String> = HashSet::new();
    let mut seen_grid: HashSet<String> = HashSet::new();

    let mut coast_edges: Vec<Edge> = Vec::new();
    let mut country_edges: Vec<Edge> = Vec::new();
    let mut province_edges: Vec<Edge> = Vec::new();
    let mut out = MapBorders::default();

    // Per-edge strip records gathered during classification; smoothed after.
    struct RawStrip {
        a: Point,
        b: Point,
        pk: String,
        tile_idx: usize,
        neighbor_idx: Option<usize>,
        coast: bool,
    }
    let mut raw_strips: Vec<RawStrip> = Vec::new();

    for (ti, tile) in tiles.iter().enumerate() {
        if tile.is_sea() {
            continue;
        }
        let [px, py] = hex_to_pixel(tile.q, tile.r, hex_size);
        let neighbors = hex_neighbors(tile.q, tile.r);
        let tile_vg = tile.visual_group_or_owner();
        for i in 0..6 {
            let (nq, nr) = neighbors[i];
            let neighbor_idx = neighbor_at(nq, nr);
            let neighbor = neighbor_idx.map(|ni| &tiles[ni]);
            let v1 = verts[i];
            let v2 = verts[(i + 1) % 6];
            let a: Point = [px + v1[0], py + v1[1]];
            let b: Point = [px + v2[0], py + v2[1]];
            let k1 = organic::vkey(a[0], a[1]);
            let k2 = organic::vkey(b[0], b[1]);
            vertex_coord.entry(k1.clone()).or_insert(a);
            vertex_coord.entry(k2.clone()).or_insert(b);

            let is_coast = neighbor.is_none_or(|n| n.is_sea());
            let neighbor_vg = neighbor.map_or("", |n| n.visual_group_or_owner());
            let class = if is_coast {
                EdgeClass::Coast
            } else if tile_vg != neighbor_vg {
                EdgeClass::Country
            } else if !tile.owner.is_empty()
                && neighbor.is_some_and(|n| n.province != tile.province)
            {
                EdgeClass::Province
            } else {
                EdgeClass::Interior
            };

            let pk = political_key(&k1, &k2);
            if class == EdgeClass::Interior {
                if seen_grid.insert(pk) {
                    out.grid_segments.push([a, b]);
                }
                continue;
            }

            // Straight fallbacks mirror React's classifiedEdges: owned coast
            // and country edges share the country style; province separate.
            // (These push once per visiting tile in React, but the stroke
            // result is identical either way — dedup below via pk.)
            // Canonical outward normal + ruggedness, computed once per edge.
            if !edge_normal.contains_key(&pk) {
                let (sa, sb) = if k1 < k2 { (&k1, &k2) } else { (&k2, &k1) };
                let pa = vertex_coord[sa.as_str()];
                let pb = vertex_coord[sb.as_str()];
                let dx = pb[0] - pa[0];
                let dy = pb[1] - pa[1];
                let len = dx.hypot(dy).max(f64::MIN_POSITIVE);
                let mut nx = -dy / len;
                let mut ny = dx / len;
                let mx = (pa[0] + pb[0]) * 0.5;
                let my = (pa[1] + pb[1]) * 0.5;
                if is_coast && (mx - px) * nx + (my - py) * ny < 0.0 {
                    nx = -nx;
                    ny = -ny;
                }
                edge_normal.insert(pk.clone(), [nx, ny]);
                edge_amp_mult.insert(pk.clone(), ruggedness_multiplier(mx, my, rugged_freq));
            }

            match class {
                EdgeClass::Coast => {
                    coast_edges.push(Edge::new(k1.clone(), k2.clone()));
                    if !tile.owner.is_empty() {
                        out.straight_country_segments.push([a, b]);
                    }
                    raw_strips.push(RawStrip {
                        a,
                        b,
                        pk,
                        tile_idx: ti,
                        neighbor_idx: None,
                        coast: true,
                    });
                }
                EdgeClass::Country => {
                    if seen_political.insert(pk.clone()) {
                        country_edges.push(Edge::new(k1.clone(), k2.clone()));
                        out.straight_country_segments.push([a, b]);
                        raw_strips.push(RawStrip {
                            a,
                            b,
                            pk,
                            tile_idx: ti,
                            neighbor_idx,
                            coast: false,
                        });
                    }
                }
                EdgeClass::Province => {
                    if seen_political.insert(pk.clone()) {
                        province_edges.push(Edge::new(k1.clone(), k2.clone()));
                        out.straight_province_segments.push([a, b]);
                    }
                }
                EdgeClass::Interior => unreachable!(),
            }
        }
    }

    let opts = AnchoredOpts {
        frequency,
        octaves: BORDER_OCTAVES,
        seed: BORDER_SEED,
        smoothing: BORDER_SMOOTHING,
        closed: false,
        // Anti-spike taper: strips and strokes share this opts value, so
        // clamped curves still coincide exactly (see `slope_clamp` docs).
        slope_clamp: BORDER_SLOPE_CLAMP,
    };

    // ── Stroke polylines: stitch each bucket and smooth with canonical
    //    normals + per-edge ruggedness amplitudes ──────────────────────────
    let smooth_bucket = |edges: &[Edge], amplitude: f64, subdiv: usize| -> Vec<Polyline> {
        let stitched = stitch_polylines(edges);
        let mut polylines = Vec::new();
        let mut smooth_keys = |keys: &[String], closed: bool| {
            if keys.len() < if closed { 3 } else { 2 } {
                return;
            }
            let pts: Vec<Point> = keys.iter().map(|k| vertex_coord[k.as_str()]).collect();
            let seg_count = if closed { pts.len() } else { pts.len() - 1 };
            let mut seg_amp = Vec::with_capacity(seg_count);
            let mut seg_normals = Vec::with_capacity(seg_count);
            for i in 0..seg_count {
                let pk = political_key(&keys[i], &keys[(i + 1) % keys.len()]);
                seg_amp.push(amplitude * edge_amp_mult.get(&pk).copied().unwrap_or(1.0));
                seg_normals.push(edge_normal.get(&pk).copied().unwrap_or([0.0, 0.0]));
            }
            let seg_sub = vec![subdiv; seg_count];
            let smoothed = smooth_polyline_anchored(
                &pts,
                &seg_amp,
                &seg_sub,
                AnchoredOpts { closed, ..opts },
                Some(&seg_normals),
            );
            if smoothed.len() >= 2 {
                polylines.push(Polyline {
                    pts: smoothed,
                    closed,
                });
            }
        };
        for keys in &stitched.closed {
            smooth_keys(keys, true);
        }
        for keys in &stitched.open {
            smooth_keys(keys, false);
        }
        polylines
    };

    out.coast_strokes = smooth_bucket(&coast_edges, coast_amp, COAST_SUBDIV);
    out.country_strokes = smooth_bucket(&country_edges, country_amp, COUNTRY_BORDER_SUBDIV);
    out.province_strokes = smooth_bucket(&province_edges, province_amp, PROVINCE_BORDER_SUBDIV);

    // ── Fill-correction strips: smooth each edge in isolation. Because the
    //    anchored pipeline smooths per segment with pinned endpoints, the
    //    single-edge curve is identical to that edge's portion of any
    //    stitched stroke above — strips and strokes coincide exactly. ──────
    for raw in raw_strips {
        let normal = edge_normal[&raw.pk];
        let mult = edge_amp_mult[&raw.pk];
        let amplitude = if raw.coast { coast_amp } else { country_amp } * mult;
        let subdiv = if raw.coast {
            COAST_SUBDIV
        } else {
            COUNTRY_BORDER_SUBDIV
        };
        let curve = smooth_polyline_anchored(
            &[raw.a, raw.b],
            &[amplitude],
            &[subdiv],
            opts,
            Some(&[normal]),
        );
        let tile = &tiles[raw.tile_idx];
        let [px, py] = hex_to_pixel(tile.q, tile.r, hex_size);
        let mx = (raw.a[0] + raw.b[0]) * 0.5;
        let my = (raw.a[1] + raw.b[1]) * 0.5;
        let outward_sign = if (mx - px) * normal[0] + (my - py) * normal[1] >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let strip = FillStrip {
            curve,
            base_a: raw.a,
            normal,
            outward_sign,
            tile_idx: raw.tile_idx,
            neighbor_idx: raw.neighbor_idx,
        };
        if raw.coast {
            out.coast_strips.push(strip);
        } else {
            out.country_strips.push(strip);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(q: i32, r: i32, terrain: &str, owner: &str, province: &str) -> MapTile {
        MapTile {
            q,
            r,
            map_width: 100,
            map_height: 100,
            terrain: terrain.to_string(),
            owner: owner.to_string(),
            owner_color: String::new(),
            nation_id: 0,
            province: province.to_string(),
            province_id: None,
            is_capital: false,
            is_country_capital: false,
            is_minor: false,
            is_incorporated_minor: false,
            incorporated_nation_id: None,
            is_anarchic: false,
            is_prospected: true,
            resource: None,
            resource_hidden: false,
            improvement_level: 0,
            max_improvement_level: 0,
            rail_links: Vec::new(),
            has_depot: false,
            has_port: false,
            has_fort: false,
            has_river: false,
            fort_level: 0,
            port_blockaded: false,
            army_unit_count: 0,
            army_firepower: 0.0,
            army_composition: None,
            naval_ship_count: 0,
            naval_firepower: 0,
            civilian_on_tile: None,
            visible: true,
            visual_group: None,
        }
    }

    #[test]
    fn two_hex_island_same_owner_two_provinces() {
        let tiles = vec![
            tile(0, 0, "Grassland", "A", "P1"),
            tile(1, 0, "Grassland", "A", "P2"),
        ];
        let borders = classify(&tiles, 24.0);

        // 10 coast edges stitch into one closed loop.
        assert_eq!(borders.coast_strokes.len(), 1);
        assert!(borders.coast_strokes[0].closed);
        assert_eq!(borders.coast_strips.len(), 10);
        // Same owner → no country border; shared edge is a province border.
        assert!(borders.country_strokes.is_empty());
        assert_eq!(borders.province_strokes.len(), 1);
        assert!(!borders.province_strokes[0].closed);
        assert_eq!(borders.straight_province_segments.len(), 1);
        // Owned coast also appears in the straight country fallback.
        assert_eq!(borders.straight_country_segments.len(), 10);
        assert!(borders.grid_segments.is_empty());
    }

    #[test]
    fn different_owners_produce_country_border_and_strips() {
        let tiles = vec![
            tile(0, 0, "Grassland", "A", "P1"),
            tile(1, 0, "Grassland", "B", "P2"),
        ];
        let borders = classify(&tiles, 24.0);
        assert_eq!(borders.country_strokes.len(), 1);
        assert!(borders.province_strokes.is_empty());
        // One deduped country strip for the shared edge, with a neighbor.
        assert_eq!(borders.country_strips.len(), 1);
        assert!(borders.country_strips[0].neighbor_idx.is_some());
        // Canonical normal is unit length; curve endpoints anchor the edge.
        let strip = &borders.country_strips[0];
        let nl = strip.normal[0].hypot(strip.normal[1]);
        assert!((nl - 1.0).abs() < 1e-9);
        let first = strip.curve[0];
        let last = strip.curve[strip.curve.len() - 1];
        let endpoints_anchor = (first[0] - strip.base_a[0]).abs() < 1e-9
            && (first[1] - strip.base_a[1]).abs() < 1e-9
            || (last[0] - strip.base_a[0]).abs() < 1e-9 && (last[1] - strip.base_a[1]).abs() < 1e-9;
        assert!(endpoints_anchor);
    }

    #[test]
    fn interior_edges_become_grid_segments() {
        // Three mutually adjacent same-province hexes: 3 interior edges.
        let tiles = vec![
            tile(0, 0, "Grassland", "A", "P"),
            tile(1, 0, "Grassland", "A", "P"),
            tile(0, 1, "Grassland", "A", "P"),
        ];
        let borders = classify(&tiles, 24.0);
        assert_eq!(borders.grid_segments.len(), 3);
        assert!(borders.province_strokes.is_empty());
    }

    #[test]
    fn wrap_axial_wraps_in_offset_space() {
        // Row 0: q ∈ [0, w). q = -1 wraps to w-1.
        assert_eq!(wrap_axial(-1, 0, 40), (39, 0));
        // Row 2 shifts by floor(2/2)=1: q ∈ [-1, 39). q = -2 wraps to 38.
        assert_eq!(wrap_axial(-2, 2, 40), (38, 2));
        assert_eq!(wrap_axial(5, 3, 0), (5, 3));
    }
}
