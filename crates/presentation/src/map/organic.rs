//! Organic border geometry — a bit-faithful Rust port of
//! `web/src/lib/mapGeometry.ts`.
//!
//! The trick (used by Civ 6, Endless Legend, etc.): leave the hex grid alone
//! for gameplay, but when rendering visible borders (coastlines, province /
//! country edges), replace straight hex-edge segments with smoothed, noise-
//! displaced polylines. All noise is sampled in WORLD SPACE so neighboring
//! edges agree at shared vertices and don't produce kinks.
//!
//! Everything here is a pure function over plain `f64` data — no Bevy types.
//! The math intentionally mirrors the JavaScript original operation-for-
//! operation (`Math.imul` → `i32::wrapping_mul`, `>>>` → `u32` shifts,
//! `Math.round` → `floor(x + 0.5)`, string vertex keys with lexicographic
//! ordering) so both frontends draw the same wiggles for the same world.

use std::collections::{HashMap, HashSet};

/// A world-space point. JS numbers are f64, so the port stays f64 internally;
/// convert at the rendering boundary with [`points_to_f32`].
pub type Point = [f64; 2];

// ── Style constants (mirroring HexMap.tsx tunables) ────────────────────────
//
// The React frontend hardcodes amplitudes in world units against its
// `HEX_SIZE = 18`. Here every amplitude is expressed as a fraction of the
// hex size so the Bevy frontend (hex size 24) gets the same look at scale.

/// Hex size the React frontend's absolute tunables were authored against.
pub const REACT_HEX_SIZE: f64 = 18.0;
/// Displacement-noise frequency (cycles per world unit at [`REACT_HEX_SIZE`]).
pub const BORDER_FREQUENCY: f64 = 0.06;
/// fBm octaves used for all border displacement.
pub const BORDER_OCTAVES: u32 = 4;
/// Chaikin passes applied per anchored segment.
pub const BORDER_SMOOTHING: u32 = 1;
/// Seed for the shared border-displacement noise field.
pub const BORDER_SEED: i32 = 1337;

/// Coastline displacement amplitude as a fraction of the hex size.
pub const COAST_AMPLITUDE_FRAC: f64 = 0.48;
/// Sub-points inserted along each coastline hex edge.
pub const COAST_SUBDIV: usize = 12;
/// Country-border displacement amplitude as a fraction of the hex size.
pub const COUNTRY_BORDER_AMPLITUDE_FRAC: f64 = 0.34;
/// Sub-points inserted along each country-border hex edge.
pub const COUNTRY_BORDER_SUBDIV: usize = 10;
/// Province-border displacement amplitude as a fraction of the hex size.
pub const PROVINCE_BORDER_AMPLITUDE_FRAC: f64 = 0.22;
/// Sub-points inserted along each province-border hex edge.
pub const PROVINCE_BORDER_SUBDIV: usize = 8;

/// Per-edge ruggedness field (second noise layer sampled at edge midpoints,
/// remapped to an amplitude multiplier).
pub const RUGGEDNESS_FREQUENCY: f64 = 0.014;
/// fBm octaves for the ruggedness field.
pub const RUGGEDNESS_OCTAVES: u32 = 2;
/// Seed for the ruggedness field.
pub const RUGGEDNESS_SEED: i32 = 9001;
/// Flattest amplitude multiplier.
pub const RUGGEDNESS_MIN: f64 = 0.35;
/// Most rugged amplitude multiplier.
pub const RUGGEDNESS_MAX: f64 = 1.55;

/// Displacement slope clamp used for map borders (see
/// [`AnchoredOpts::slope_clamp`]). Because segment endpoints are pinned
/// anchors, an unclamped displacement right next to an endpoint can fold the
/// curve almost perpendicular to the hex edge — the "border spike" artifact.
/// Limiting |d| to `slope_clamp * len * min(t, 1-t)` tapers the wiggle into
/// each anchor (max flare-out angle `atan(slope_clamp)`) while leaving the
/// mid-edge displacement untouched.
pub const BORDER_SLOPE_CLAMP: f64 = 1.5;

/// Border-noise frequency scaled so the wiggle wavelength stays a constant
/// number of hexes regardless of hex size.
#[must_use]
pub fn border_frequency(hex_size: f64) -> f64 {
    BORDER_FREQUENCY * REACT_HEX_SIZE / hex_size
}

// ── Rivers (card #539) ─────────────────────────────────────────────────────

/// River meander amplitude as a fraction of the hex size. Kept well under
/// half the hex inradius (`√3/2 · hex_size / 2`) so a displaced segment stays
/// inside the corridor of its two hexes and never wanders into a neighboring
/// hex's features.
pub const RIVER_AMPLITUDE_FRAC: f64 = 0.18;
/// Sub-points inserted along each hex-center-to-hex-center river segment.
pub const RIVER_SUBDIV: usize = 10;
/// fBm octaves for river displacement.
pub const RIVER_OCTAVES: u32 = 3;
/// Chaikin passes per river segment.
pub const RIVER_SMOOTHING: u32 = 2;
/// Seed for the river-displacement noise field. Distinct from
/// [`BORDER_SEED`] / [`RUGGEDNESS_SEED`] so rivers meander independently of
/// coastlines; constant across rebuilds so rivers never wiggle frame to
/// frame.
pub const RIVER_SEED: i32 = 4547;
/// Anchor-taper clamp for rivers (see [`AnchoredOpts::slope_clamp`]). The
/// displacement tapers to zero at both hex-center anchors, so consecutive
/// segments meeting at a shared hex center stay continuous and spike-free.
pub const RIVER_SLOPE_CLAMP: f64 = 1.0;

/// Meandering river course between two hex centers: the straight segment
/// `a → b` displaced by seeded world-space fBm noise and Chaikin-smoothed,
/// with both endpoints pinned exactly. Pure and deterministic — the same
/// endpoints always produce the same curve, and because the endpoints are
/// hard anchors, chains of segments sharing a hex center join continuously.
///
/// Callers must feed each undirected river edge in ONE canonical direction:
/// reversing `a` and `b` mirrors the displacement (the walk normal flips
/// while the noise field does not).
#[must_use]
pub fn river_polyline(a: Point, b: Point, hex_size: f64) -> Vec<Point> {
    smooth_polyline_anchored(
        &[a, b],
        &[RIVER_AMPLITUDE_FRAC * hex_size],
        &[RIVER_SUBDIV],
        AnchoredOpts {
            frequency: border_frequency(hex_size),
            octaves: RIVER_OCTAVES,
            seed: RIVER_SEED,
            smoothing: RIVER_SMOOTHING,
            closed: false,
            slope_clamp: RIVER_SLOPE_CLAMP,
        },
        None,
    )
}

/// Ruggedness-noise frequency scaled the same way as [`border_frequency`].
#[must_use]
pub fn ruggedness_frequency(hex_size: f64) -> f64 {
    RUGGEDNESS_FREQUENCY * REACT_HEX_SIZE / hex_size
}

/// Ruggedness multiplier at an edge midpoint: fBm remapped from [-1, 1]
/// into [[`RUGGEDNESS_MIN`], [`RUGGEDNESS_MAX`]]. Multiply the class
/// amplitude by this for the final per-edge amplitude.
#[must_use]
pub fn ruggedness_multiplier(mx: f64, my: f64, frequency: f64) -> f64 {
    let raw = fbm(
        mx * frequency,
        my * frequency,
        RUGGEDNESS_OCTAVES,
        RUGGEDNESS_SEED,
    );
    let t = ((raw + 1.0) * 0.5).clamp(0.0, 1.0);
    RUGGEDNESS_MIN + (RUGGEDNESS_MAX - RUGGEDNESS_MIN) * t
}

// ── JS semantics helpers ───────────────────────────────────────────────────

/// ECMAScript `ToInt32` of an integral f64 (modular reduction into i32).
fn js_to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let m = x.trunc().rem_euclid(4_294_967_296.0);
    let m = if m >= 2_147_483_648.0 {
        m - 4_294_967_296.0
    } else {
        m
    };
    m as i32
}

/// `Math.round` (half toward +∞), returned as i64 for key formatting.
fn js_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

// ── Deterministic value noise ──────────────────────────────────────────────

/// Integer-lattice hash → [-1, 1]. Bit-faithful to the JS original
/// (`Math.imul` wrapping multiplies, `>>>` unsigned shifts).
fn hash2(ix: i32, iy: i32, seed: i32) -> f64 {
    let mut h = ix.wrapping_mul(374_761_393)
        ^ iy.wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(-1_640_531_535);
    h = (h ^ ((h as u32) >> 13) as i32).wrapping_mul(1_274_126_177);
    h ^= ((h as u32) >> 16) as i32;
    (f64::from(h as u32) / 4_294_967_295.0) * 2.0 - 1.0
}

fn smootherstep(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// 2D value noise, output in roughly [-1, 1]. Cheap and deterministic.
#[must_use]
pub fn noise2(x: f64, y: f64, seed: i32) -> f64 {
    let ixf = x.floor();
    let iyf = y.floor();
    let fx = x - ixf;
    let fy = y - iyf;
    let ix = js_to_int32(ixf);
    let iy = js_to_int32(iyf);
    let v00 = hash2(ix, iy, seed);
    let v10 = hash2(ix.wrapping_add(1), iy, seed);
    let v01 = hash2(ix, iy.wrapping_add(1), seed);
    let v11 = hash2(ix.wrapping_add(1), iy.wrapping_add(1), seed);
    let u = smootherstep(fx);
    let v = smootherstep(fy);
    let a = v00 + (v10 - v00) * u;
    let b = v01 + (v11 - v01) * u;
    a + (b - a) * v
}

/// Fractal Brownian motion: sum of decaying-amplitude noise octaves.
/// `octaves` must be >= 1 (the JS original returns NaN for 0; here 0 octaves
/// yields 0.0 instead).
#[must_use]
pub fn fbm(x: f64, y: f64, octaves: u32, seed: i32) -> f64 {
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for i in 0..octaves {
        let oct_seed = seed.wrapping_add((i as i32).wrapping_mul(101));
        sum += amp * noise2(x * freq, y * freq, oct_seed);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    if norm == 0.0 { 0.0 } else { sum / norm }
}

// ── Vertex-key helpers ─────────────────────────────────────────────────────
//
// Each hex vertex in world space is quantised to a stable string key so that
// edges shared by two hexes produce the same key for their endpoints. String
// keys (not integer tuples) are kept on purpose: the stitcher orders edge
// endpoints lexicographically exactly like the JS original, so loop walks
// start from the same vertex on both frontends.

/// Quantise a world-space coordinate to a string key (1e-3 resolution).
#[must_use]
pub fn vkey(x: f64, y: f64) -> String {
    format!("{}_{}", js_round(x * 1000.0), js_round(y * 1000.0))
}

// ── Polyline/loop stitching ────────────────────────────────────────────────

/// An undirected hex-edge segment between two vertex keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// First endpoint vertex key (see [`vkey`]).
    pub a: String,
    /// Second endpoint vertex key.
    pub b: String,
}

impl Edge {
    /// Convenience constructor.
    pub fn new(a: impl Into<String>, b: impl Into<String>) -> Self {
        Self {
            a: a.into(),
            b: b.into(),
        }
    }
}

/// Result of [`stitch_polylines`]: ordered vertex-key sequences.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StitchedPolylines {
    /// Closed loops; the starting vertex is NOT repeated at the end.
    pub closed: Vec<Vec<String>>,
    /// Open polylines (degree-1 endpoints, or chains off junction vertices).
    pub open: Vec<Vec<String>>,
}

fn edge_key(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

fn walk(
    start: &str,
    adj: &HashMap<String, Vec<String>>,
    unused: &mut HashSet<(String, String)>,
) -> Vec<String> {
    let mut path = vec![start.to_owned()];
    let mut cur = start.to_owned();
    let mut prev: Option<String> = None;
    loop {
        let nbs: &[String] = match adj.get(&cur) {
            Some(v) => v,
            None => &[],
        };
        let mut next: Option<String> = None;
        for nb in nbs {
            if prev.as_deref() == Some(nb.as_str()) && nbs.len() > 1 {
                continue;
            }
            if unused.contains(&edge_key(&cur, nb)) {
                next = Some(nb.clone());
                break;
            }
        }
        let Some(nx) = next else { break };
        unused.remove(&edge_key(&cur, &nx));
        path.push(nx.clone());
        prev = Some(cur);
        cur = nx;
    }
    path
}

/// Stitch a set of undirected edges into polylines. Each vertex should have
/// degree 1, 2, or rarely higher. Degree-2 vertices become interior points of
/// a polyline; degree-1 vertices become endpoints of open polylines; degree-2
/// loops with no degree-1 vertex become closed loops.
///
/// Iteration order matches the JS original (insertion-ordered maps/sets), so
/// loop start vertices and polyline ordering are reproduced exactly.
#[must_use]
pub fn stitch_polylines(edges: &[Edge]) -> StitchedPolylines {
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut vertex_order: Vec<String> = Vec::new();
    for e in edges {
        if !adj.contains_key(e.a.as_str()) {
            vertex_order.push(e.a.clone());
        }
        adj.entry(e.a.clone()).or_default().push(e.b.clone());
        if !adj.contains_key(e.b.as_str()) {
            vertex_order.push(e.b.clone());
        }
        adj.entry(e.b.clone()).or_default().push(e.a.clone());
    }
    let mut unused: HashSet<(String, String)> = HashSet::new();
    let mut unused_order: Vec<(String, String)> = Vec::new();
    for e in edges {
        let k = edge_key(&e.a, &e.b);
        if unused.insert(k.clone()) {
            unused_order.push(k);
        }
    }

    let mut closed: Vec<Vec<String>> = Vec::new();
    let mut open: Vec<Vec<String>> = Vec::new();

    // Start open walks at degree-1 vertices first.
    let mut started_open: HashSet<String> = HashSet::new();
    for v in &vertex_order {
        let Some(nbs) = adj.get(v) else { continue };
        if nbs.len() != 1 || started_open.contains(v) {
            continue;
        }
        let has_unused = nbs.iter().any(|nb| unused.contains(&edge_key(v, nb)));
        if !has_unused {
            continue;
        }
        let path = walk(v, &adj, &mut unused);
        started_open.insert(v.clone());
        if path.len() >= 2 {
            started_open.insert(path[path.len() - 1].clone());
        }
        open.push(path);
    }

    // Remaining unused edges form closed loops (or chains hanging off higher-
    // degree vertices — treat those as open too). The JS `Set.values().next()`
    // yields the earliest-inserted remaining key; a cursor over the insertion
    // order reproduces that (edges are only ever removed, never re-added).
    let mut cursor = 0;
    while !unused.is_empty() {
        while cursor < unused_order.len() && !unused.contains(&unused_order[cursor]) {
            cursor += 1;
        }
        if cursor >= unused_order.len() {
            break;
        }
        let start_key = unused_order[cursor].0.clone();
        let mut path = walk(&start_key, &adj, &mut unused);
        if path.len() > 2 && path[0] == path[path.len() - 1] {
            path.pop();
            closed.push(path);
        } else {
            open.push(path);
        }
    }
    StitchedPolylines { closed, open }
}

// ── Displacement + smoothing ───────────────────────────────────────────────

/// Options for [`displace_along_normal`].
#[derive(Debug, Clone, Copy)]
pub struct DisplaceOpts {
    /// Sub-points inserted per segment (interior points = `subdiv - 1`).
    pub subdiv: usize,
    /// Displacement amplitude in world units (use `*_AMPLITUDE_FRAC * hex_size`).
    pub amplitude: f64,
    /// Noise frequency in cycles per world unit (see [`border_frequency`]).
    pub frequency: f64,
    /// fBm octaves.
    pub octaves: u32,
    /// Noise seed.
    pub seed: i32,
    /// Treat the polyline as a closed loop.
    pub closed: bool,
}

/// Subdivide each segment of a polyline and perturb the interior sub-points
/// along the edge normal using world-space fBm noise. Segment endpoints are
/// left fixed so shared vertices between edges remain consistent.
#[must_use]
pub fn displace_along_normal(pts: &[Point], opts: DisplaceOpts) -> Vec<Point> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let mut out: Vec<Point> = Vec::new();
    let seg_count = if opts.closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        out.push(a);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if len == 0.0 {
            continue;
        }
        let nx = -dy / len;
        let ny = dx / len;
        for k in 1..opts.subdiv {
            let t = k as f64 / opts.subdiv as f64;
            let px = a[0] + dx * t;
            let py = a[1] + dy * t;
            let d = opts.amplitude
                * fbm(
                    px * opts.frequency,
                    py * opts.frequency,
                    opts.octaves,
                    opts.seed,
                );
            out.push([px + nx * d, py + ny * d]);
        }
    }
    if !opts.closed {
        out.push(pts[n - 1]);
    }
    out
}

/// Chaikin corner-cutting: each pass replaces every interior vertex with two
/// points at 1/4 and 3/4 along its incoming/outgoing segments. Produces C1
/// smoothing in 2 iterations for next-to-no cost.
#[must_use]
pub fn chaikin(pts: &[Point], iterations: u32, closed: bool) -> Vec<Point> {
    let mut cur = pts.to_vec();
    for _ in 0..iterations {
        let n = cur.len();
        if n < 2 {
            break;
        }
        let mut next: Vec<Point> = Vec::new();
        if !closed {
            next.push(cur[0]);
        }
        let seg_count = if closed { n } else { n - 1 };
        for i in 0..seg_count {
            let a = cur[i];
            let b = cur[(i + 1) % n];
            next.push([a[0] * 0.75 + b[0] * 0.25, a[1] * 0.75 + b[1] * 0.25]);
            next.push([a[0] * 0.25 + b[0] * 0.75, a[1] * 0.25 + b[1] * 0.75]);
        }
        if !closed {
            next.push(cur[n - 1]);
        }
        cur = next;
    }
    cur
}

/// Options shared by [`smooth_polyline_anchored`] and
/// [`displace_along_normal_mixed`].
#[derive(Debug, Clone, Copy)]
pub struct AnchoredOpts {
    /// Noise frequency in cycles per world unit.
    pub frequency: f64,
    /// fBm octaves.
    pub octaves: u32,
    /// Noise seed.
    pub seed: i32,
    /// Chaikin passes per segment (0 = displacement only).
    pub smoothing: u32,
    /// Treat the polyline as a closed loop.
    pub closed: bool,
    /// Anchor-taper clamp on the displacement: at parameter `t` along a
    /// segment of length `len`, |d| is limited to
    /// `slope_clamp * len * min(t, 1 - t)`. `0.0` disables the clamp
    /// (bit-faithful to the TS original); [`BORDER_SLOPE_CLAMP`] is the map
    /// borders' anti-spike setting.
    pub slope_clamp: f64,
}

/// Clamp a displacement to the anchor-taper envelope (see
/// [`AnchoredOpts::slope_clamp`]).
fn clamp_displacement(d: f64, slope_clamp: f64, len: f64, t: f64) -> f64 {
    if slope_clamp <= 0.0 {
        return d;
    }
    let limit = slope_clamp * len * t.min(1.0 - t);
    d.clamp(-limit, limit)
}

/// Full "organic edge" pipeline that keeps every hex vertex as a hard anchor:
/// for each segment `pts[i] -> pts[(i+1)%n]`, generate its sub-polyline with
/// noise-displaced sub-points and Chaikin-smooth that sub-polyline as OPEN
/// (endpoints fixed), then concatenate. Because each segment is smoothed in
/// isolation, two polylines that share a segment produce an identical
/// sub-curve for it — which is what makes two neighbouring nations' clip
/// boundaries agree exactly along their shared border.
///
/// `seg_amp[i]` / `seg_subdiv[i]` apply to segment `i` (missing entries fall
/// back to 0 amplitude / subdiv 4, clamped to >= 2, exactly like the JS
/// `?? 0` / `?? 4` defaults). If `seg_normals` is supplied, those unit
/// normals are used instead of the walk-direction perpendicular —
/// pre-computed canonical normals are required when the same edge is visited
/// by more than one polyline and must displace identically in both.
#[must_use]
pub fn smooth_polyline_anchored(
    pts: &[Point],
    seg_amp: &[f64],
    seg_subdiv: &[usize],
    opts: AnchoredOpts,
    seg_normals: Option<&[Point]>,
) -> Vec<Point> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let mut out: Vec<Point> = Vec::new();
    let seg_count = if opts.closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let amp = seg_amp.get(i).copied().unwrap_or(0.0);
        let sub = seg_subdiv.get(i).copied().unwrap_or(4).max(2);
        let mut seg: Vec<Point> = vec![a];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if amp > 0.0 && len > 0.0 {
            let (nx, ny) = match seg_normals.and_then(|ns| ns.get(i)) {
                Some(nv) => (nv[0], nv[1]),
                None => (-dy / len, dx / len),
            };
            for k in 1..sub {
                let t = k as f64 / sub as f64;
                let px = a[0] + dx * t;
                let py = a[1] + dy * t;
                let d = amp
                    * fbm(
                        px * opts.frequency,
                        py * opts.frequency,
                        opts.octaves,
                        opts.seed,
                    );
                let d = clamp_displacement(d, opts.slope_clamp, len, t);
                seg.push([px + nx * d, py + ny * d]);
            }
        }
        seg.push(b);
        let smoothed = if opts.smoothing > 0 {
            chaikin(&seg, opts.smoothing, false)
        } else {
            seg
        };
        if out.is_empty() {
            out.extend_from_slice(&smoothed);
        } else {
            out.extend_from_slice(&smoothed[1..]);
        }
    }
    out
}

/// Options for [`organic_polyline`], mirroring the TS defaults.
#[derive(Debug, Clone, Copy)]
pub struct OrganicOpts {
    /// Sub-points per segment (TS default 8).
    pub subdiv: usize,
    /// Displacement amplitude in world units.
    pub amplitude: f64,
    /// Noise frequency in cycles per world unit.
    pub frequency: f64,
    /// fBm octaves (TS default 2).
    pub octaves: u32,
    /// Noise seed (TS default 1).
    pub seed: i32,
    /// Whole-polyline Chaikin passes (TS default 2).
    pub smoothing: u32,
    /// Treat the polyline as a closed loop (TS default false).
    pub closed: bool,
}

impl OrganicOpts {
    /// Options with the TS defaults for everything except the two
    /// always-explicit knobs.
    #[must_use]
    pub fn new(amplitude: f64, frequency: f64) -> Self {
        Self {
            subdiv: 8,
            amplitude,
            frequency,
            octaves: 2,
            seed: 1,
            smoothing: 2,
            closed: false,
        }
    }
}

/// Convenience: full "organic edge" pipeline — displace then Chaikin smooth.
#[must_use]
pub fn organic_polyline(pts: &[Point], opts: OrganicOpts) -> Vec<Point> {
    let displaced = displace_along_normal(
        pts,
        DisplaceOpts {
            subdiv: opts.subdiv,
            amplitude: opts.amplitude,
            frequency: opts.frequency,
            octaves: opts.octaves,
            seed: opts.seed,
            closed: opts.closed,
        },
    );
    chaikin(&displaced, opts.smoothing, opts.closed)
}

/// Like [`displace_along_normal`] but with per-segment amplitude/subdiv and
/// optional canonical normals, without per-segment smoothing. Use when a
/// polyline walks around a region whose boundary is made of different edge
/// types (e.g. coast vs nation-nation) that should displace by different
/// amounts so the polygon aligns with separately-drawn strokes.
/// (`opts.smoothing` is ignored here, matching the TS function signature.)
#[must_use]
pub fn displace_along_normal_mixed(
    pts: &[Point],
    seg_amp: &[f64],
    seg_subdiv: &[usize],
    opts: AnchoredOpts,
    seg_normals: Option<&[Point]>,
) -> Vec<Point> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let mut out: Vec<Point> = Vec::new();
    let seg_count = if opts.closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let amp = seg_amp.get(i).copied().unwrap_or(0.0);
        let sub = seg_subdiv.get(i).copied().unwrap_or(4).max(2);
        out.push(a);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if len == 0.0 || amp == 0.0 {
            continue;
        }
        let (nx, ny) = match seg_normals.and_then(|ns| ns.get(i)) {
            Some(nv) => (nv[0], nv[1]),
            None => (-dy / len, dx / len),
        };
        for k in 1..sub {
            let t = k as f64 / sub as f64;
            let px = a[0] + dx * t;
            let py = a[1] + dy * t;
            let d = amp
                * fbm(
                    px * opts.frequency,
                    py * opts.frequency,
                    opts.octaves,
                    opts.seed,
                );
            let d = clamp_displacement(d, opts.slope_clamp, len, t);
            out.push([px + nx * d, py + ny * d]);
        }
    }
    if !opts.closed {
        out.push(pts[n - 1]);
    }
    out
}

/// Convert f64 points to f32 pairs at the rendering boundary.
#[must_use]
pub fn points_to_f32(pts: &[Point]) -> Vec<[f32; 2]> {
    pts.iter().map(|p| [p[0] as f32, p[1] as f32]).collect()
}

// ── Reference tests ────────────────────────────────────────────────────────
//
// Expected values were captured by executing the TypeScript original
// (web/src/lib/mapGeometry.ts transpiled with esbuild --format=cjs, run under
// node v22) on the exact fixtures below. JSON.stringify emits the shortest
// round-trip representation of each f64, so the literals are bit-exact
// doubles; the 1e-9 tolerance only absorbs possible last-ulp differences
// between JS Math.hypot and Rust f64::hypot.
#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn assert_pts_eq(actual: &[Point], expected: &[Point], what: &str) {
        assert_eq!(actual.len(), expected.len(), "{what}: point count");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (a[0] - e[0]).abs() < EPS && (a[1] - e[1]).abs() < EPS,
                "{what}: point {i}: got {a:?}, expected {e:?}"
            );
        }
    }

    // 10-point open polyline used for displacement fixtures.
    const PTS_A: &[Point] = &[
        [0.0, 0.0],
        [14.0, 6.0],
        [30.0, 2.0],
        [42.0, 18.0],
        [55.0, 25.0],
        [61.0, 40.0],
        [50.0, 58.0],
        [33.0, 63.0],
        [18.0, 52.0],
        [5.0, 38.0],
    ];

    fn pts_b() -> Vec<Point> {
        PTS_A[..6].to_vec()
    }

    const PTS_C: &[Point] = &[
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [20.0, 10.0],
        [20.0, 20.0],
        [30.0, 25.0],
    ];

    const PTS_D: &[Point] = &[[0.0, 0.0], [18.0, 4.0], [31.0, 17.0], [45.0, 12.0]];

    const PTS_E: &[Point] = &[
        [0.0, 0.0],
        [15.0, 3.0],
        [22.0, 16.0],
        [9.0, 24.0],
        [-4.0, 13.0],
    ];
    const SEG_AMP_E: &[f64] = &[8.64, 6.12, 0.0, 3.96, 8.64];
    const SEG_SUB_E: &[usize] = &[4, 3, 2, 3, 4];
    const SEG_NORMALS_E: &[Point] = &[[0.0, 1.0], [0.6, 0.8], [1.0, 0.0], [-0.8, 0.6], [0.0, -1.0]];

    // (x, y, seed, expected)
    const NOISE2_CASES: &[(f64, f64, i32, f64)] = &[
        (0.3, 0.7, 1337, 0.4082296348684479),
        (12.34, -5.67, 1337, -0.3330233556708671),
        (-3.21, 8.88, 42, -0.4198302307962095),
        (100.5, 200.25, 1, -0.6946221659530449),
        (5.0, -7.0, 1337, 0.30405632809364613),
        (0.0, 0.0, 9001, 0.724436677462523),
    ];

    // (x, y, octaves, seed, expected)
    const FBM_CASES: &[(f64, f64, u32, i32, f64)] = &[
        (0.3, 0.7, 4, 1337, 0.4303141435086379),
        (12.34, -5.67, 4, 1337, 0.09393587223424078),
        (-3.21, 8.88, 4, 1337, 0.16186258073019502),
        (2.5, -1.25, 2, 9001, 0.049883812769109546),
        (7.7, 3.3, 1, 1, -0.25157924010156607),
    ];

    const DISPLACED_OPEN: &[Point] = &[
        [0.0, 0.0],
        [5.5949263936562526, -0.16593936297570222],
        [8.63653211540008, 5.625869508510925],
        [14.0, 6.0],
        [18.924586387761, 3.0316788843773357],
        [24.484737262189466, 2.6056157154245447],
        [30.0, 2.0],
        [34.59451426000465, 6.887447638329844],
        [36.971424423446024, 13.438098349082148],
        [42.0, 18.0],
        [46.10575576722527, 20.755977384676886],
        [49.26441885992886, 25.27084116489401],
        [55.0, 25.0],
        [54.894540058160835, 30.84218397673567],
        [56.9567553437534, 35.81729786249864],
        [61.0, 40.0],
        [54.92508997572799, 44.528295725907846],
        [51.05202718378925, 50.402164760463805],
        [50.0, 58.0],
        [44.19047754060034, 59.18095697137449],
        [38.019090687511934, 59.131575004207235],
        [33.0, 63.0],
        [29.080216432380208, 57.860310925542144],
        [24.55701686522714, 53.543461850447834],
        [18.0, 52.0],
        [15.02134918914721, 46.07541384817283],
        [8.597595690707346, 43.34985162053366],
        [5.0, 38.0],
    ];

    const DISPLACED_CLOSED: &[Point] = &[
        [0.0, 0.0],
        [6.91943872708779, 3.1879763034618214],
        [14.0, 6.0],
        [21.919365253040592, 3.677461012162366],
        [30.0, 2.0],
        [36.32344118566032, 9.75741911075476],
        [42.0, 18.0],
        [47.96698306572112, 22.489888592232216],
        [55.0, 25.0],
        [56.511611811737794, 33.09535527530488],
        [61.0, 40.0],
        [30.729792684828357, 19.649566155636755],
    ];

    const CHAIKIN_OPEN: &[Point] = &[
        [0.0, 0.0],
        [0.625, 0.0],
        [1.875, 0.0],
        [3.75, 0.0],
        [6.25, 0.0],
        [8.125, 0.625],
        [9.375, 1.875],
        [10.0, 3.75],
        [10.0, 6.25],
        [10.625, 8.125],
        [11.875, 9.375],
        [13.75, 10.0],
        [16.25, 10.0],
        [18.125, 10.625],
        [19.375, 11.875],
        [20.0, 13.75],
        [20.0, 16.25],
        [20.625, 18.4375],
        [21.875, 20.3125],
        [23.75, 21.875],
        [26.25, 23.125],
        [28.125, 24.0625],
        [29.375, 24.6875],
        [30.0, 25.0],
    ];

    const CHAIKIN_CLOSED: &[Point] = &[
        [3.75, 0.0],
        [6.25, 0.0],
        [8.125, 0.625],
        [9.375, 1.875],
        [10.0, 3.75],
        [10.0, 6.25],
        [10.625, 8.125],
        [11.875, 9.375],
        [13.75, 10.0],
        [16.25, 10.0],
        [18.125, 10.625],
        [19.375, 11.875],
        [20.0, 13.75],
        [20.0, 16.25],
        [20.625, 18.4375],
        [21.875, 20.3125],
        [23.75, 21.875],
        [26.25, 23.125],
        [26.25, 22.5],
        [23.75, 20.0],
        [18.75, 15.625],
        [11.25, 9.375],
        [6.25, 4.6875],
        [3.75, 1.5625],
    ];

    const ORGANIC_OPEN: &[Point] = &[
        [0.0, 0.0],
        [0.38158302573765257, 0.053709717513896735],
        [1.1447490772129578, 0.1611291525416902],
        [2.2894981544259156, 0.3222583050833804],
        [3.815830257376526, 0.5370971751389673],
        [5.317643667013078, 0.8622701651078162],
        [6.794938383335572, 1.2977772749899268],
        [8.247714406344008, 1.8436185047852993],
        [9.675971736038386, 2.499793854493934],
        [11.133517375147921, 3.0241718118343544],
        [12.620351323672615, 3.416752376806561],
        [14.136473581612469, 3.6775355494105537],
        [15.68188414896748, 3.8065213296463325],
        [17.159076059690513, 4.126793346283061],
        [18.568049313781565, 4.63835159932074],
        [19.908803911240632, 5.341196088759369],
        [21.181339852067715, 6.235326814598949],
        [22.358705959436726, 7.224627373896606],
        [23.440902233347654, 8.309097766652343],
        [24.4279286738005, 9.48873799286616],
        [25.31978528079528, 10.763548052538054],
        [26.26007959908589, 11.989920400914107],
        [27.24881162867235, 13.167855037994318],
        [28.285981369554648, 14.297351963778686],
        [29.371588821732786, 15.37841117826721],
        [30.504558476281787, 16.16091163896337],
        [31.68489033320164, 16.644853345867162],
        [32.91258439249235, 16.83023629897859],
        [34.187640654153924, 16.717060498297656],
        [35.470949570892046, 16.626992131831045],
        [36.7625111427067, 16.56003119957876],
        [38.06232536959791, 16.5161777015408],
        [39.370392251565654, 16.495431637717168],
        [40.580661627134134, 16.200852555975587],
        [41.693133496303346, 15.632440456316054],
        [42.7078078590733, 14.790195338738574],
        [43.624684715443976, 13.674117203243146],
        [44.31234235772199, 12.837058601621573],
        [44.770780785907334, 12.279019533873857],
        [45.0, 12.0],
    ];

    const ORGANIC_CLOSED: &[Point] = &[
        [2.098097397621095, -0.06222726111588833],
        [3.496828996035158, -0.10371210185981389],
        [4.735978052454695, 0.2271623220501561],
        [5.815544566879704, 0.9303960106140217],
        [6.735528539310188, 2.0059889638317827],
        [7.495929969746144, 3.4539411817034398],
        [8.401447785360606, 4.56328850082525],
        [9.452081986153576, 5.334030921197212],
        [10.64783257212505, 5.766168442819328],
        [11.988699543275029, 5.859701065691596],
        [13.302136420872577, 5.744330463119381],
        [14.588143204917692, 5.420056635102683],
        [15.846719895410374, 4.886879581641501],
        [17.077866492350626, 4.144799302735835],
        [18.348735869707593, 3.561610145497036],
        [19.659328027481276, 3.137312109925104],
        [21.009642965671677, 2.8719051960200392],
        [22.39968068427879, 2.765389403781841],
        [23.786912894347285, 2.6476515773891585],
        [25.17133959587716, 2.518691716841992],
        [26.552960788868415, 2.3785098221403405],
        [27.93177647332105, 2.2271058932842043],
        [29.253045377910816, 2.4190184240377173],
        [30.516767502637713, 2.9542474144008795],
        [31.72294284750174, 3.8327928643736913],
        [32.87157141250291, 5.054654773956153],
        [33.88159972146887, 6.380466875565017],
        [34.75302777439962, 7.810229169200285],
        [35.48585557129517, 9.343941654861958],
        [36.080083112155506, 10.981604332550035],
        [36.84003974133539, 12.494970193998459],
        [37.76572545883481, 13.884039239207226],
        [38.857140264653765, 15.148811468176344],
        [40.11428415879226, 16.289286880905806],
        [41.313751814847706, 17.31689202699521],
        [42.45554323282012, 18.23162690644455],
        [43.53965841270948, 19.03349151925383],
        [44.56609735451579, 19.722485865423053],
        [45.53334300416451, 20.52141061131354],
        [46.44139536165561, 21.430265756925294],
        [47.290254426989115, 22.449051302258308],
        [48.07992020016501, 23.577767247312586],
        [49.03064335130138, 24.40737663329742],
        [50.142423880398226, 24.937879460212812],
        [51.415261787455535, 25.169275728058757],
        [52.849157072473325, 25.10156543683525],
        [53.91798728987172, 25.415919216963605],
        [54.621752439650706, 26.112337068443814],
        [54.96045252181031, 27.190818991275876],
        [54.934087536350525, 28.651364985459793],
        [55.04320225260522, 30.057719098957918],
        [55.28779667057439, 31.409881331770244],
        [55.66787079025805, 32.70785168389678],
        [56.18342461165619, 33.951630155337526],
        [56.822792768720205, 35.14588289251192],
        [57.5859752614501, 36.29060989541996],
        [58.47297208984588, 37.38581116406165],
        [59.48378325390752, 38.431486698436984],
        [59.00254638171848, 38.33439068153562],
        [57.02926147327875, 37.09452311335756],
        [53.563928528588335, 34.71188399390279],
        [48.606547547647224, 31.18647332317132],
        [43.56254345062822, 27.79316290445864],
        [38.43191623753131, 24.531952737764747],
        [33.21466590835651, 21.402842823089653],
        [27.910792463103817, 18.40583316043335],
        [22.68570098571275, 15.288680996788067],
        [17.539391476183305, 12.0513863321538],
        [12.471863934515483, 8.693949166530558],
        [7.48311836070929, 5.2163694999183345],
        [4.091242079958161, 2.597813539773186],
        [2.296235092262096, 0.8382812860951115],
    ];

    const ANCHORED_CLOSED: &[Point] = &[
        [0.0, 0.0],
        [1.0632969763994367, -0.4414848819971835],
        [3.18989092919831, -1.3244546459915505],
        [5.050883989643778, -0.8794199482188909],
        [6.646276157735841, 0.8936192113207955],
        [8.33195052770724, 2.2152473614637955],
        [10.107907099557977, 3.08546450221011],
        [11.996914039112509, 3.39042980443745],
        [13.998971346370837, 3.130143268145817],
        [15.0, 3.0],
        [15.876735041194157, 3.9253477983313507],
        [17.63020512358247, 5.776043394994053],
        [19.014690874309697, 7.825422862551188],
        [20.03019229337583, 10.073486201002758],
        [20.903457252181674, 12.398138402671407],
        [21.634485750727222, 14.799379467557134],
        [22.0, 16.0],
        [18.75, 18.0],
        [12.25, 22.0],
        [9.0, 24.0],
        [8.130738627319143, 22.83033919801677],
        [6.392215881957433, 20.491017594050305],
        [4.693333244478362, 18.104848589858904],
        [3.0340907148819323, 15.671832185442568],
        [0.6533520875627881, 14.091492987425799],
        [-2.4488826374790706, 13.363830995808598],
        [-4.0, 13.0],
        [-3.6527432800938318, 12.217425144586514],
        [-2.9582298402814953, 10.65227543375954],
        [-3.136033634759771, 8.818720420073916],
        [-4.186154663528658, 6.716760103529644],
        [-4.6753671732876825, 4.78738702360379],
        [-4.603671164036845, 3.0306011802963555],
        [-3.425867369558569, 1.6141561939819788],
        [-1.1419557898528563, 0.5380520646606596],
        [0.0, 0.0],
    ];

    const ANCHORED_CLOSED_NORMALS: &[Point] = &[
        [0.0, 0.0],
        [0.9375, -0.4539412374116988],
        [2.8125, -1.3618237122350965],
        [4.6875, -0.9154020541208235],
        [6.5625, 0.8853237369311201],
        [8.4375, 2.2256988188701996],
        [10.3125, 3.105723191696416],
        [12.1875, 3.409301533582143],
        [14.0625, 3.1364338445273807],
        [15.0, 3.0],
        [15.383393750157854, 3.8167472224326957],
        [16.150181250473565, 5.450241667298087],
        [17.168414367818865, 7.4189969348696],
        [18.438093102193758, 9.723013025147235],
        [19.804699352035904, 12.15626580271454],
        [21.2682331173453, 14.718755267571513],
        [22.0, 16.0],
        [18.75, 18.0],
        [12.25, 22.0],
        [9.0, 24.0],
        [7.651538366443944, 23.282179558500374],
        [4.954615099331833, 21.846538675501126],
        [2.208597525840049, 20.447718522286632],
        [-0.5865143540314077, 19.085719098856888],
        [-2.488052720475352, 17.053539540356514],
        [-3.496017573491784, 14.351179846785506],
        [-4.0, 13.0],
        [-3.75, 12.085743507109225],
        [-3.25, 10.25723052132767],
        [-2.75, 9.341394238351151],
        [-2.25, 9.338234658179665],
        [-1.75, 8.748215025745782],
        [-1.25, 7.571335341049508],
        [-0.75, 5.237171624026028],
        [-0.25, 1.7457238746753427],
        [0.0, 0.0],
    ];

    const ANCHORED_OPEN: &[Point] = &[
        [0.0, 0.0],
        [0.26582424409985916, -0.11037122049929587],
        [0.7974727322995775, -0.3311136614978876],
        [1.594945464599155, -0.6622273229957752],
        [2.658242440998592, -1.1037122049929589],
        [3.655139194309677, -1.2131959715483855],
        [4.585635724532411, -0.9906786226620559],
        [5.449732031666794, -0.4361601583339693],
        [6.247428115712825, 0.4503594214358738],
        [7.067694750228691, 1.2240262488565454],
        [7.91053193521439, 1.8848403239280456],
        [8.775939670669924, 2.432801646650374],
        [9.663917956595293, 2.8679102170235313],
        [10.58015883444661, 3.161705827766945],
        [11.524662304223876, 3.3141884788806153],
        [12.49742836592709, 3.325358170364542],
        [13.498457019556255, 3.1952149022187255],
        [14.249228509778128, 3.0976074511093628],
        [14.74974283659271, 3.0325358170364543],
        [15.0, 3.0],
        [15.21918376029854, 3.231336949582838],
        [15.657551280895618, 3.694010848748513],
        [16.315102561791235, 4.388021697497026],
        [17.19183760298539, 5.313369495828377],
        [17.976326561264276, 6.288388261883337],
        [18.66856943662789, 7.313077995661905],
        [19.26856622907623, 8.38743869716408],
        [19.776316938609295, 9.511470366389865],
        [20.248508533077292, 10.65464925141992],
        [20.685141012480212, 11.816975352254245],
        [21.086214376818063, 12.998448668892838],
        [21.451728626090834, 14.199069201335703],
        [21.725864313045417, 15.099534600667852],
        [21.908621437681806, 15.699844866889283],
        [22.0, 16.0],
        [21.1875, 16.5],
        [19.5625, 17.5],
        [17.125, 19.0],
        [13.875, 21.0],
        [11.4375, 22.5],
        [9.8125, 23.5],
        [9.0, 24.0],
        [8.782684656829787, 23.707584799504193],
        [8.348053970489357, 23.122754398512576],
        [7.696107940978716, 22.245508797025153],
        [6.82684656829786, 21.075847995041922],
        [5.967495222587665, 19.894475343002455],
        [5.118053903848129, 18.701390840906754],
        [4.2785226120792546, 17.49659448875482],
        [3.4489013472810397, 16.280086286546652],
        [2.4389060580521464, 15.276747385938375],
        [1.2485367443925741, 14.486577786929992],
        [-0.12220659369767661, 13.9095774895215],
        [-1.673323956218606, 13.545746493712898],
        [-2.836661978109303, 13.272873246856449],
        [-3.6122206593697674, 13.09095774895215],
        [-4.0, 13.0],
    ];

    #[test]
    fn noise2_and_fbm_match_ts_reference() {
        for &(x, y, seed, expected) in NOISE2_CASES {
            let got = noise2(x, y, seed);
            assert!(
                (got - expected).abs() < EPS,
                "noise2({x}, {y}, {seed}) = {got}, expected {expected}"
            );
        }
        for &(x, y, octaves, seed, expected) in FBM_CASES {
            let got = fbm(x, y, octaves, seed);
            assert!(
                (got - expected).abs() < EPS,
                "fbm({x}, {y}, {octaves}, {seed}) = {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn displace_along_normal_matches_ts_reference() {
        let open = displace_along_normal(
            PTS_A,
            DisplaceOpts {
                subdiv: 3,
                amplitude: 8.64,
                frequency: 0.06,
                octaves: 4,
                seed: 1337,
                closed: false,
            },
        );
        assert_pts_eq(&open, DISPLACED_OPEN, "displaced open");

        let closed = displace_along_normal(
            &pts_b(),
            DisplaceOpts {
                subdiv: 2,
                amplitude: 6.12,
                frequency: 0.06,
                octaves: 4,
                seed: 1337,
                closed: true,
            },
        );
        assert_pts_eq(&closed, DISPLACED_CLOSED, "displaced closed");
    }

    #[test]
    fn chaikin_matches_ts_reference() {
        assert_pts_eq(&chaikin(PTS_C, 2, false), CHAIKIN_OPEN, "chaikin open");
        assert_pts_eq(&chaikin(PTS_C, 2, true), CHAIKIN_CLOSED, "chaikin closed");
    }

    #[test]
    fn organic_polyline_matches_ts_reference() {
        let open = organic_polyline(
            PTS_D,
            OrganicOpts {
                subdiv: 3,
                amplitude: 6.12,
                frequency: 0.06,
                octaves: 4,
                seed: 1337,
                smoothing: 2,
                closed: false,
            },
        );
        assert_pts_eq(&open, ORGANIC_OPEN, "organic open");

        let closed = organic_polyline(
            &pts_b(),
            OrganicOpts {
                subdiv: 3,
                amplitude: 8.64,
                frequency: 0.06,
                octaves: 4,
                seed: 1337,
                smoothing: 2,
                closed: true,
            },
        );
        assert_pts_eq(&closed, ORGANIC_CLOSED, "organic closed");
    }

    #[test]
    fn smooth_polyline_anchored_matches_ts_reference() {
        let base = AnchoredOpts {
            frequency: 0.06,
            octaves: 4,
            seed: 1337,
            smoothing: 1,
            closed: true,
            slope_clamp: 0.0,
        };
        let closed = smooth_polyline_anchored(PTS_E, SEG_AMP_E, SEG_SUB_E, base, None);
        assert_pts_eq(&closed, ANCHORED_CLOSED, "anchored closed");

        let with_normals =
            smooth_polyline_anchored(PTS_E, SEG_AMP_E, SEG_SUB_E, base, Some(SEG_NORMALS_E));
        assert_pts_eq(
            &with_normals,
            ANCHORED_CLOSED_NORMALS,
            "anchored closed + normals",
        );

        let open = smooth_polyline_anchored(
            PTS_E,
            SEG_AMP_E,
            SEG_SUB_E,
            AnchoredOpts {
                smoothing: 2,
                closed: false,
                ..base
            },
            None,
        );
        assert_pts_eq(&open, ANCHORED_OPEN, "anchored open");
    }

    #[test]
    fn vkey_and_stitch_polylines_match_ts_reference() {
        assert_eq!(vkey(12.3456, -7.8912), "12346_-7891");
        assert_eq!(vkey(-0.0001, 0.0004999), "0_0");
        assert_eq!(vkey(31.176914536239792, -18.0), "31177_-18000");

        let e = |x1: f64, y1: f64, x2: f64, y2: f64| Edge::new(vkey(x1, y1), vkey(x2, y2));
        let edges = vec![
            // closed square loop, deliberately shuffled order
            e(10.0, 10.0, 0.0, 10.0),
            e(0.0, 0.0, 10.0, 0.0),
            e(10.0, 0.0, 10.0, 10.0),
            e(0.0, 10.0, 0.0, 0.0),
            // open chain
            e(20.0, 0.0, 30.0, 0.0),
            e(30.0, 0.0, 40.0, 5.0),
            // Y-junction: degree-3 vertex at (60, 0)
            e(50.0, 0.0, 60.0, 0.0),
            e(60.0, 0.0, 70.0, 5.0),
            e(60.0, 0.0, 70.0, -5.0),
        ];
        let got = stitch_polylines(&edges);
        assert_eq!(
            got.closed,
            vec![vec![
                "0_10000".to_owned(),
                "10000_10000".to_owned(),
                "10000_0".to_owned(),
                "0_0".to_owned(),
            ]],
            "closed loops"
        );
        assert_eq!(
            got.open,
            vec![
                vec![
                    "20000_0".to_owned(),
                    "30000_0".to_owned(),
                    "40000_5000".to_owned(),
                ],
                vec![
                    "50000_0".to_owned(),
                    "60000_0".to_owned(),
                    "70000_5000".to_owned(),
                ],
                vec!["70000_-5000".to_owned(), "60000_0".to_owned()],
            ],
            "open polylines"
        );
    }

    /// The anti-spike clamp bounds every sub-point's displacement from the
    /// straight chord by `slope_clamp * len * min(t, 1-t)`, tapering into the
    /// pinned anchors, while `slope_clamp: 0.0` stays bit-faithful.
    #[test]
    fn slope_clamp_tapers_displacement_into_anchors() {
        // One long horizontal segment with a huge amplitude so the raw noise
        // displacement dwarfs the clamp envelope near the anchors.
        let pts: &[Point] = &[[0.0, 0.0], [40.0, 0.0]];
        let opts = AnchoredOpts {
            frequency: 0.06,
            octaves: 4,
            seed: 1337,
            smoothing: 0,
            closed: false,
            slope_clamp: BORDER_SLOPE_CLAMP,
        };
        let sub = 12;
        let out = smooth_polyline_anchored(pts, &[100.0], &[sub], opts, Some(&[[0.0, 1.0]]));
        assert_eq!(out.len(), sub + 1);
        for (k, p) in out.iter().enumerate() {
            let t = k as f64 / sub as f64;
            let limit = BORDER_SLOPE_CLAMP * 40.0 * t.min(1.0 - t) + 1e-9;
            assert!(
                p[1].abs() <= limit,
                "sub-point {k}: displacement {} exceeds envelope {limit}",
                p[1]
            );
        }
        // Somewhere mid-edge the noise must actually displace (the clamp is
        // a taper, not a flattener).
        assert!(out.iter().any(|p| p[1].abs() > 1.0));
    }

    /// Card #539: river meanders must be deterministic (same endpoints →
    /// bit-identical curve on every rebuild), pinned at both hex-center
    /// anchors, stay within the hex corridor, and actually meander.
    #[test]
    fn river_polyline_is_deterministic_pinned_and_bounded() {
        let hex = 24.0;
        // A representative hex-center-to-hex-center segment (pointy-top
        // neighbors are √3·hex apart) plus an arbitrary diagonal one.
        let cases: &[(Point, Point)] = &[
            ([100.0, -50.0], [100.0 + 3.0_f64.sqrt() * hex, -50.0]),
            ([0.0, 0.0], [3.0_f64.sqrt() / 2.0 * hex, -1.5 * hex]),
        ];
        for &(a, b) in cases {
            let first = river_polyline(a, b, hex);
            let second = river_polyline(a, b, hex);
            assert_eq!(first, second, "same endpoints must give the same curve");
            assert!(first.len() > 2, "curve must gain meander sub-points");

            // Endpoints pinned exactly: joints between chained segments and
            // the coastline mouth stay continuous.
            assert_eq!(first[0], a, "start anchor must be pinned");
            assert_eq!(*first.last().unwrap(), b, "end anchor must be pinned");

            // Every point stays within the meander corridor around the
            // chord (Chaikin only shrinks toward the displaced polyline, and
            // |fbm| <= 1, so amplitude bounds the perpendicular deviation).
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let len = dx.hypot(dy);
            let (nx, ny) = (-dy / len, dx / len);
            let max_dev = first
                .iter()
                .map(|p| ((p[0] - a[0]) * nx + (p[1] - a[1]) * ny).abs())
                .fold(0.0_f64, f64::max);
            let bound = RIVER_AMPLITUDE_FRAC * hex + EPS;
            assert!(
                max_dev <= bound,
                "deviation {max_dev} exceeds corridor bound {bound}"
            );
            // ... and the course is not just the straight line.
            assert!(
                max_dev > 0.5,
                "river must visibly meander (max deviation {max_dev})"
            );
        }
    }

    #[test]
    fn amplitude_fractions_match_react_absolute_values() {
        // React hardcodes 8.64 / 6.12 / 3.96 world units at HEX_SIZE 18.
        assert!((COAST_AMPLITUDE_FRAC * REACT_HEX_SIZE - 8.64).abs() < EPS);
        assert!((COUNTRY_BORDER_AMPLITUDE_FRAC * REACT_HEX_SIZE - 6.12).abs() < EPS);
        assert!((PROVINCE_BORDER_AMPLITUDE_FRAC * REACT_HEX_SIZE - 3.96).abs() < EPS);
    }
}
