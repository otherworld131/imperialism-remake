// Geometry helpers for drawing a hex-based map without a visibly-hex silhouette.
//
// The trick (used by Civ 6, Endless Legend, etc.): leave the hex grid alone
// for gameplay, but when rendering visible borders (coastlines, province /
// country edges), replace straight hex-edge segments with smoothed, noise-
// displaced polylines. All noise is sampled in WORLD SPACE so neighboring
// edges agree at shared vertices and don't produce kinks.

// ── Deterministic value noise ──────────────────────────────────────────────

function hash2(ix: number, iy: number, seed: number): number {
  let h = Math.imul(ix | 0, 374761393) ^ Math.imul(iy | 0, 668265263) ^ Math.imul(seed | 0, -1640531535);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  h = h ^ (h >>> 16);
  return ((h >>> 0) / 4294967295) * 2 - 1; // -> [-1, 1]
}

function smootherstep(t: number): number {
  return t * t * t * (t * (t * 6 - 15) + 10);
}

/** 2D value noise, output in roughly [-1, 1]. Cheap and deterministic. */
export function noise2(x: number, y: number, seed = 1): number {
  const ix = Math.floor(x), iy = Math.floor(y);
  const fx = x - ix, fy = y - iy;
  const v00 = hash2(ix, iy, seed);
  const v10 = hash2(ix + 1, iy, seed);
  const v01 = hash2(ix, iy + 1, seed);
  const v11 = hash2(ix + 1, iy + 1, seed);
  const u = smootherstep(fx);
  const v = smootherstep(fy);
  const a = v00 + (v10 - v00) * u;
  const b = v01 + (v11 - v01) * u;
  return a + (b - a) * v;
}

/** Fractal Brownian motion: sum of decaying-amplitude noise octaves. */
export function fbm(x: number, y: number, octaves = 2, seed = 1): number {
  let amp = 1, freq = 1, sum = 0, norm = 0;
  for (let i = 0; i < octaves; i++) {
    sum += amp * noise2(x * freq, y * freq, seed + i * 101);
    norm += amp;
    amp *= 0.5;
    freq *= 2;
  }
  return sum / norm;
}

// ── Vertex-key helpers ─────────────────────────────────────────────────────
//
// Each hex vertex in world space is quantised to a stable string key so that
// edges shared by two hexes produce the same key for their endpoints.

export type Vec2 = [number, number];

/** Quantise a world-space coordinate to a string key. 1e-3 is plenty given hex size 18. */
export function vKey(x: number, y: number): string {
  return `${Math.round(x * 1000)}_${Math.round(y * 1000)}`;
}

// ── Polyline/loop stitching ────────────────────────────────────────────────

export interface Edge {
  a: string; // vertex key
  b: string; // vertex key
}

/**
 * Stitch a set of undirected edges into polylines. Each vertex should have
 * degree 1, 2, or rarely higher. Degree-2 vertices become interior points of
 * a polyline; degree-1 vertices become endpoints of open polylines; degree-2
 * loops with no degree-1 vertex become closed loops.
 *
 * Returns polylines as ordered arrays of vertex keys. `closed[i]` members
 * do not repeat the starting vertex at the end.
 */
export function stitchPolylines(edges: Edge[]): { closed: string[][]; open: string[][] } {
  const edgeKey = (a: string, b: string) => (a < b ? `${a}|${b}` : `${b}|${a}`);
  const adj = new Map<string, string[]>();
  for (const { a, b } of edges) {
    if (!adj.has(a)) adj.set(a, []);
    if (!adj.has(b)) adj.set(b, []);
    adj.get(a)!.push(b);
    adj.get(b)!.push(a);
  }
  const unused = new Set<string>();
  for (const { a, b } of edges) unused.add(edgeKey(a, b));

  const closed: string[][] = [];
  const open: string[][] = [];

  const walk = (start: string): string[] => {
    const path = [start];
    let cur = start;
    let prev: string | null = null;
    while (true) {
      const nbs = adj.get(cur) ?? [];
      let next: string | null = null;
      for (const nb of nbs) {
        if (nb === prev && (adj.get(cur) ?? []).length > 1) continue;
        if (unused.has(edgeKey(cur, nb))) { next = nb; break; }
      }
      if (next == null) break;
      unused.delete(edgeKey(cur, next));
      path.push(next);
      prev = cur;
      cur = next;
    }
    return path;
  };

  // Start open walks at degree-1 vertices first.
  const startedOpen = new Set<string>();
  for (const [v, nbs] of adj) {
    if (nbs.length !== 1) continue;
    if (startedOpen.has(v)) continue;
    const hasUnused = nbs.some(nb => unused.has(edgeKey(v, nb)));
    if (!hasUnused) continue;
    const path = walk(v);
    startedOpen.add(v);
    if (path.length >= 2) startedOpen.add(path[path.length - 1]);
    open.push(path);
  }

  // Remaining unused edges form closed loops (or chains hanging off higher-
  // degree vertices — treat those as open too).
  while (unused.size > 0) {
    const any = unused.values().next().value!;
    const startKey = any.split('|')[0];
    const path = walk(startKey);
    if (path.length > 2 && path[0] === path[path.length - 1]) {
      path.pop();
      closed.push(path);
    } else {
      open.push(path);
    }
  }
  return { closed, open };
}

// ── Displacement + smoothing ───────────────────────────────────────────────

/**
 * Subdivide each segment of a polyline and perturb the interior sub-points
 * along the edge normal using world-space fBm noise. Segment endpoints are
 * left fixed so shared vertices between edges remain consistent.
 */
export function displaceAlongNormal(
  pts: Vec2[],
  opts: { subdiv: number; amplitude: number; frequency: number; octaves: number; seed: number; closed: boolean },
): Vec2[] {
  const { subdiv, amplitude, frequency, octaves, seed, closed } = opts;
  const n = pts.length;
  if (n < 2) return pts.slice();
  const out: Vec2[] = [];
  const segCount = closed ? n : n - 1;
  for (let i = 0; i < segCount; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % n];
    out.push(a);
    const dx = b[0] - a[0], dy = b[1] - a[1];
    const len = Math.hypot(dx, dy);
    if (len === 0) continue;
    const nx = -dy / len, ny = dx / len;
    for (let k = 1; k < subdiv; k++) {
      const t = k / subdiv;
      const px = a[0] + dx * t;
      const py = a[1] + dy * t;
      const d = amplitude * fbm(px * frequency, py * frequency, octaves, seed);
      out.push([px + nx * d, py + ny * d]);
    }
  }
  if (!closed) out.push(pts[n - 1]);
  return out;
}

/**
 * Chaikin corner-cutting: each pass replaces every interior vertex with two
 * points at 1/4 and 3/4 along its incoming/outgoing segments. Produces C1
 * smoothing in 2 iterations for next-to-no cost.
 */
export function chaikin(pts: Vec2[], iterations = 2, closed = false): Vec2[] {
  let cur = pts;
  for (let k = 0; k < iterations; k++) {
    const n = cur.length;
    if (n < 2) break;
    const next: Vec2[] = [];
    if (!closed) next.push(cur[0]);
    const segCount = closed ? n : n - 1;
    for (let i = 0; i < segCount; i++) {
      const a = cur[i];
      const b = cur[(i + 1) % n];
      next.push([a[0] * 0.75 + b[0] * 0.25, a[1] * 0.75 + b[1] * 0.25]);
      next.push([a[0] * 0.25 + b[0] * 0.75, a[1] * 0.25 + b[1] * 0.75]);
    }
    if (!closed) next.push(cur[n - 1]);
    cur = next;
  }
  return cur;
}

/**
 * Convenience: full "organic edge" pipeline — displace then Chaikin smooth.
 */
export function organicPolyline(
  pts: Vec2[],
  opts: {
    subdiv?: number;
    amplitude: number;
    frequency: number;
    octaves?: number;
    seed?: number;
    smoothing?: number;
    closed?: boolean;
  },
): Vec2[] {
  const subdiv = opts.subdiv ?? 8;
  const octaves = opts.octaves ?? 2;
  const seed = opts.seed ?? 1;
  const smoothing = opts.smoothing ?? 2;
  const closed = opts.closed ?? false;
  const displaced = displaceAlongNormal(pts, {
    subdiv, amplitude: opts.amplitude, frequency: opts.frequency, octaves, seed, closed,
  });
  return chaikin(displaced, smoothing, closed);
}

/**
 * Like displaceAlongNormal but per-segment amplitude/subdiv. `segAmp[i]` and
 * `segSubdiv[i]` apply to segment pts[i] -> pts[(i+1)%n]. Use this when a
 * polyline walks around a region whose boundary is made of different edge
 * types (e.g. coast vs nation-nation) that should displace by different
 * amounts so the polygon aligns with separately-drawn strokes.
 */
export function displaceAlongNormalMixed(
  pts: Vec2[],
  segAmp: number[],
  segSubdiv: number[],
  opts: { frequency: number; octaves: number; seed: number; closed: boolean },
): Vec2[] {
  const { frequency, octaves, seed, closed } = opts;
  const n = pts.length;
  if (n < 2) return pts.slice();
  const out: Vec2[] = [];
  const segCount = closed ? n : n - 1;
  for (let i = 0; i < segCount; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % n];
    const amp = segAmp[i] ?? 0;
    const sub = Math.max(2, segSubdiv[i] ?? 4);
    out.push(a);
    const dx = b[0] - a[0], dy = b[1] - a[1];
    const len = Math.hypot(dx, dy);
    if (len === 0 || amp === 0) continue;
    const nx = -dy / len, ny = dx / len;
    for (let k = 1; k < sub; k++) {
      const t = k / sub;
      const px = a[0] + dx * t;
      const py = a[1] + dy * t;
      const d = amp * fbm(px * frequency, py * frequency, octaves, seed);
      out.push([px + nx * d, py + ny * d]);
    }
  }
  if (!closed) out.push(pts[n - 1]);
  return out;
}
