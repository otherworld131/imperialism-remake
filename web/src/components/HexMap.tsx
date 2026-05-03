import { useRef, useEffect, useLayoutEffect, useState, useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';
import type { TileData, MapMode, DiplomacyOverlay, MilitaryOverlayEntry, ArmyUnitDetail, ValidMoveTargets, NavyMarker, SeaZone } from '../wasm';
import { computeNationLabels } from '../lib/nationLabels';
import { stitchPolylines, vKey, smoothPolylineAnchored, fbm, type Vec2 } from '../lib/mapGeometry';
import HexTooltip from './HexTooltip';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

// Perf instrumentation toggle. Flip in DevTools console with
//   localStorage.setItem('hexmap.perf', '1'); location.reload();
// to log mapGeometry / classifiedEdges / render timings. Off by default so
// production users don't see console noise.
const PERF_LOG = typeof window !== 'undefined' && window.localStorage?.getItem('hexmap.perf') === '1';
const perfMark = (label: string): (() => void) => {
  if (!PERF_LOG) return () => {};
  const t0 = performance.now();
  return () => console.log(`[hexmap] ${label}: ${(performance.now() - t0).toFixed(2)}ms`);
};

// ─── Organic-border tunables ────────────────────────────────────────────
// All knobs for the "non-hex looking" map rendering live here. Tweak these
// and the effect appears immediately on next render — no other code needs
// to change. See web/src/lib/mapGeometry.ts for the underlying math.

// Displacement noise — shared by coast + country + province smoothing.
// One seed + one frequency means a coastline and a country border passing
// near the same point displace by correlated amounts, so they read as
// drawn on the same map.
//   BORDER_FREQUENCY: cycles per world unit. Higher = tighter wiggles.
//   BORDER_OCTAVES:   fBm octaves. More = more fine detail.
//   BORDER_SMOOTHING: Chaikin passes per segment. Larger = softer corners.
//   BORDER_SEED:      change for a different wiggle pattern.
const BORDER_FREQUENCY = 0.06;
const BORDER_OCTAVES = 4;
const BORDER_SMOOTHING = 1;
const BORDER_SEED = 1337;

// Per-class amplitude (in world units, relative to HEX_SIZE) and the
// number of sub-points inserted along each hex edge. Larger amplitude =
// wavier; more subdivisions = smoother curve.
const COAST_AMPLITUDE = HEX_SIZE * 0.48;
const COAST_SUBDIV = 12;

const COUNTRY_BORDER_AMPLITUDE = HEX_SIZE * 0.34;
const COUNTRY_BORDER_SUBDIV = 10;

const PROVINCE_BORDER_AMPLITUDE = HEX_SIZE * 0.22;
const PROVINCE_BORDER_SUBDIV = 8;

// ─── Per-edge ruggedness ────────────────────────────────────────────────
// A second noise field (independent of BORDER_*) sampled at each edge
// midpoint and remapped to a multiplier. Some regions end up flatter,
// others more rugged, with smooth transitions. Multiplies the class
// amplitude above; final per-edge amp = AMPLITUDE * mult.
//
// To tune the look:
//   * Raise RUGGEDNESS_MAX / lower RUGGEDNESS_MIN → more contrast between
//     flat and rugged areas.
//   * Lower RUGGEDNESS_FREQUENCY → larger flat/rugged regions; higher →
//     more frequent alternation.
//   * Change RUGGEDNESS_SEED → different ruggedness layout.
const RUGGEDNESS_FREQUENCY = 0.014;
const RUGGEDNESS_OCTAVES = 2;
const RUGGEDNESS_SEED = 9001;
const RUGGEDNESS_MIN = 0.35; // flattest multiplier
const RUGGEDNESS_MAX = 1.55; // most rugged multiplier

// ─── Preview highlight ──────────────────────────────────────────────────
// Stroke used for the "selected nation" outline on the new-game preview.
const PREVIEW_HIGHLIGHT_COLOR = '#ff2a2a';
const PREVIEW_HIGHLIGHT_WIDTH = 3.5;

const TERRAIN_COLORS: Record<string, string> = {
  Grassland: '#a8b860',
  Hills:     '#9a8a68',
  Forest:    '#3a7a3a',
  Mountain:  '#7a7068',
  Desert:    '#d8c888',
  Swamp:     '#5a7a5a',
  Tundra:    '#b8c8d0',
  Sea:       '#4a88b8',
};

const NATION_COLORS: Record<string, string> = {
  Yellow: '#ffd900', Orange: '#ff8c00', LightBlue: '#66b3ff',
  Red: '#e62626', Green: '#1abf1a', Purple: '#a633d9',
  Blue: '#3359e6',
  Crimson: '#b00020', Magenta: '#d913a8', Forest: '#1f5b2c',
  Gold: '#d4a52a', Aqua: '#00b8c4', Violet: '#8a2be2',
  BurntOrange: '#cc5500', HotPink: '#ff44a0', Turquoise: '#14b89c',
  Slate: '#5a6e8c', Mauve: '#b07ab0', Sage: '#7a9b6a',
  Mustard: '#b88a00',
  Gray: '#999', Brown: '#8c5926',
  Pink: '#ff80b3', Teal: '#00b3a6', Olive: '#808000',
  Maroon: '#8c001a', Navy: '#00008c', Cyan: '#00cccc',
  Lime: '#73d900', Coral: '#ff8059', Lavender: '#b380e6',
  Tan: '#ccb380', Salmon: '#ff8c73', Khaki: '#bfb366',
  Indigo: '#4d0080', Beige: '#e8d8b0',
};

const DIPLO_STATUS_COLORS: Record<string, string> = {
  'Alliance': '#2ecc40',
  'NAP': '#7fdbff',
  'At War': '#ff4136',
  'Neutral': '#aaaaaa',
};

const MAP_MODE_LABELS: Record<MapMode, string> = {
  terrain: 'Terrain',
  political: 'Political',
  diplomatic: 'Diplomatic',
  relationship: 'Relationship',
  military: 'Military',
  naval: 'Naval',
};

// Returns a resource icon based on the resource overlay (separate from terrain)
function getResourceIcon(tile: TileData): string | null {
  if (!tile.resource) return null;
  switch (tile.resource) {
    case 'Grain':     return '\u{1F33E}';
    case 'Fruit':     return '\u{1F34E}';
    case 'Cotton':    return '\u{1F331}';
    case 'Wool':      return '\u{1F411}';
    case 'Timber':    return '\u{1FAB5}';
    case 'Livestock': return '\u{1F404}';
    case 'Horses':    return '\u{1F434}';
    case 'Coal':      return '\u26AB';        // ⚫ black circle
    case 'Iron':      return '\u{1F518}';     // 🔘 radio button (grey ring)
    case 'Gold':      return '\u{1F4B0}';     // 💰 money bag (original)
    case 'Gems':      return '\u{1F48E}';
    case 'Oil':       return '\u{1F6E2}\uFE0F';
    default:          return null;
  }
}

function politicalFill(nationHex: string): string {
  const c = parseInt(nationHex.slice(1), 16);
  const r = (c >> 16) & 0xff, g = (c >> 8) & 0xff, b = c & 0xff;
  return `rgb(${Math.min(255, r + Math.round((255 - r) * 0.45))},${Math.min(255, g + Math.round((255 - g) * 0.45))},${Math.min(255, b + Math.round((255 - b) * 0.45))})`;
}

/** Lighter shade for incorporated minor nation provinces (blends 65% toward white). */
function incorporatedFill(nationHex: string): string {
  const c = parseInt(nationHex.slice(1), 16);
  const r = (c >> 16) & 0xff, g = (c >> 8) & 0xff, b = c & 0xff;
  return `rgb(${Math.min(255, r + Math.round((255 - r) * 0.65))},${Math.min(255, g + Math.round((255 - g) * 0.65))},${Math.min(255, b + Math.round((255 - b) * 0.65))})`;
}

function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
}

function axialRound(q: number, r: number): [number, number] {
  const s = -q - r;
  let rq = Math.round(q);
  let rr = Math.round(r);
  const rs = Math.round(s);
  const dq = Math.abs(rq - q);
  const dr = Math.abs(rr - r);
  const ds = Math.abs(rs - s);
  if (dq > dr && dq > ds) {
    rq = -rr - rs;
  } else if (dr > ds) {
    rr = -rq - rs;
  }
  return [rq, rr];
}

function pixelToHex(px: number, py: number): [number, number] {
  const r = py / (HEX_SIZE * 1.5);
  const q = px / (HEX_SIZE * SQRT3) - r / 2;
  return axialRound(q, r);
}

function drawHexagon(ctx: CanvasRenderingContext2D, cx: number, cy: number, size: number) {
  ctx.beginPath();
  for (let i = 0; i < 6; i++) {
    const angle = (Math.PI / 180) * (60 * i - 30);
    const x = cx + size * Math.cos(angle);
    const y = cy + size * Math.sin(angle);
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  }
  ctx.closePath();
}

// Precompute the 6 vertex offsets for a hex of given size
function hexVertices(size: number): [number, number][] {
  const verts: [number, number][] = [];
  for (let i = 0; i < 6; i++) {
    const angle = (Math.PI / 180) * (60 * i - 30);
    verts.push([size * Math.cos(angle), size * Math.sin(angle)]);
  }
  return verts;
}

function hexNeighbors(q: number, r: number): [number, number][] {
  return [
    [q + 1, r],     // E  → edge 0
    [q, r + 1],     // SE → edge 1
    [q - 1, r + 1], // SW → edge 2
    [q - 1, r],     // W  → edge 3
    [q, r - 1],     // NW → edge 4
    [q + 1, r - 1], // NE → edge 5
  ];
}

function tintColor(terrainHex: string, nationHex: string, amount: number): string {
  const tc = parseInt(terrainHex.slice(1), 16);
  const nc = parseInt(nationHex.slice(1), 16);
  const tr = (tc >> 16) & 0xff, tg = (tc >> 8) & 0xff, tb = tc & 0xff;
  const nr = (nc >> 16) & 0xff, ng = (nc >> 8) & 0xff, nb = nc & 0xff;
  return `rgb(${Math.round(tr * (1 - amount) + nr * amount)},${Math.round(tg * (1 - amount) + ng * amount)},${Math.round(tb * (1 - amount) + nb * amount)})`;
}

/** Interpolate a relationship score (-100..+100) to a color: red → gray → green */
function scoreToColor(score: number): string {
  const t = (score + 100) / 200; // 0..1
  let r: number, g: number, b: number;
  if (t < 0.5) {
    // red to gray
    const s = t / 0.5;
    r = Math.round(220 + (160 - 220) * s);
    g = Math.round(40 + (160 - 40) * s);
    b = Math.round(40 + (160 - 40) * s);
  } else {
    // gray to green
    const s = (t - 0.5) / 0.5;
    r = Math.round(160 + (40 - 160) * s);
    g = Math.round(160 + (200 - 160) * s);
    b = Math.round(160 + (40 - 160) * s);
  }
  return `rgb(${r},${g},${b})`;
}

/** Interpolate a strength score (-100..+100) to a color: red → yellow → green */
function strengthToColor(score: number): string {
  const t = (score + 100) / 200; // 0..1
  let r: number, g: number, b: number;
  if (t < 0.5) {
    // red to yellow
    const s = t / 0.5;
    r = Math.round(220 + (200 - 220) * s);
    g = Math.round(40 + (200 - 40) * s);
    b = Math.round(40 + (40 - 40) * s);
  } else {
    // yellow to green
    const s = (t - 0.5) / 0.5;
    r = Math.round(200 + (40 - 200) * s);
    g = Math.round(200 + (200 - 200) * s);
    b = Math.round(40 + (40 - 40) * s);
  }
  return `rgb(${r},${g},${b})`;
}

/** Blend a base hex color with an overlay rgba */
function blendWithOverlay(baseHex: string, overlayR: number, overlayG: number, overlayB: number, alpha: number): string {
  const c = parseInt(baseHex.slice(1), 16);
  const br = (c >> 16) & 0xff, bg = (c >> 8) & 0xff, bb = c & 0xff;
  const r = Math.round(br * (1 - alpha) + overlayR * alpha);
  const g = Math.round(bg * (1 - alpha) + overlayG * alpha);
  const b = Math.round(bb * (1 - alpha) + overlayB * alpha);
  return `rgb(${r},${g},${b})`;
}

interface PendingMoveArrow {
  unit_id: number;
  source_province_id: number;
  dest_province_id: number;
}

interface Props {
  tiles: TileData[];
  mapMode: MapMode;
  diplomacyOverlay: DiplomacyOverlay | null;
  militaryOverlay: MilitaryOverlayEntry[] | null;
  onMapModeChange: (mode: MapMode) => void;
  onTileClick?: (tile: TileData) => void;
  onTileHover?: (tile: TileData | null) => void;
  navyMarkers?: NavyMarker[];
  seaZones?: SeaZone[];
  selectedNavyKey?: string | null;
  onNavyMarkerClick?: (marker: NavyMarker | null) => void;
  onNavyMarkerHover?: (marker: NavyMarker | null) => void;
  /** Optional slot to render mode-specific strips (diplomatic / military) inside
   *  the tile tooltip. The parent receives the hovered tile and returns a node. */
  renderTooltipModeExtras?: (tile: TileData) => ReactNode;
  showHiddenResources?: boolean;
  showAiCivilians?: boolean;
  showResources?: boolean;
  showTransportNetwork?: boolean;
  showArmies?: boolean;
  pendingMoves?: PendingMoveArrow[];
  validMoveTargets?: ValidMoveTargets | null;
  isMovementMode?: boolean;
  isDeployMode?: boolean;
  deployableTiles?: Set<string>;
  disableFogOfWar?: boolean;
  organicBorders?: boolean;
  hideHexGrid?: boolean;
  scale?: number;
  offset?: { x: number; y: number };
  onScaleChange?: (scale: number) => void;
  onOffsetChange?: (offset: { x: number; y: number }) => void;
  highlightedNationId?: number | null;
  /** nation_id → full government title (e.g., "Kingdom of Pram"). Used by tooltip. */
  governmentTitleByNationId?: Record<number, string>;
  /** Key of the currently selected tile ("q,r") — used to blink its troop indicator. */
  selectedTileKey?: string | null;
  /** When true, zoom is locked to the minimum fit-scale (map fills canvas, no zoom in/out). */
  lockZoom?: boolean;
  /** When true, render consulate/embassy emoji markers on nation label centroids. */
  showDiplomacyMarkers?: boolean;
  /** When true, the map-mode dropup only offers Terrain and Political. */
  limitedMapModes?: boolean;
  /** When true, the cursor changes to crosshair to signal the user should click a nation. */
  isDiplomacyTargetMode?: boolean;
}

const CIVILIAN_EMOJI: Record<string, string> = {
  Farmer: '\u{1F33E}',       // 🌾
  Miner: '\u26CF\uFE0F',     // ⛏️
  Engineer: '\u{1F527}',     // 🔧
  Forester: '\u{1FAA3}',     // 🪓
  Rancher: '\u{1F920}',      // 🤠
  Driller: '\u{1F6E2}\uFE0F', // 🛢️
  Prospector: '\u{1F50D}',   // 🔍
};

// Construction-in-progress emoji shown next to a working civilian. Engineer
// build tasks get a specific glyph; other civilians show a generic marker.
const IN_PROGRESS_EMOJI_BY_TASK: Record<string, string> = {
  Railroad: '\u{1F6A7}',     // 🚧
  Depot: '\u{1F3D7}\uFE0F',  // 🏗️
  Port: '\u{2693}\uFE0F',    // ⚓
};
const IN_PROGRESS_EMOJI_BY_CIV: Record<string, string> = {
  Farmer: '\u{1F33E}',       // 🌾
  Rancher: '\u{1F33E}',      // 🌾
  Forester: '\u{1FAA3}',     // 🪓
  Miner: '\u2692\uFE0F',     // ⚒️
  Driller: '\u2692\uFE0F',   // ⚒️
  Prospector: '\u{1F50D}',   // 🔍
};
const IN_PROGRESS_FALLBACK = '\u231B'; // ⌛

export { navyMarkerKey, navyMarkerOffset } from './HexMap.helpers';
import { NAVY_MARKER_RADIUS, navyMarkerKey, navyMarkerOffset } from './HexMap.helpers';

export default function HexMap({
  tiles, mapMode, diplomacyOverlay, militaryOverlay,
  onMapModeChange, onTileClick, onTileHover, showHiddenResources = false, showAiCivilians = false,
  showResources = true, showTransportNetwork = true, showArmies = true,
  pendingMoves = [], validMoveTargets, isMovementMode = false,
  isDeployMode = false, deployableTiles, disableFogOfWar = false,
  organicBorders = true,
  hideHexGrid = false,
  scale: scaleProp, offset: offsetProp, onScaleChange, onOffsetChange,
  highlightedNationId = null,
  navyMarkers = [], seaZones = [], selectedNavyKey = null, onNavyMarkerClick, onNavyMarkerHover,
  renderTooltipModeExtras,
  governmentTitleByNationId,
  selectedTileKey = null,
  lockZoom = false,
  showDiplomacyMarkers = false,
  limitedMapModes = false,
  isDiplomacyTargetMode = false,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Use props if provided (controlled mode), otherwise use local state (uncontrolled)
  const [localOffset, setLocalOffset] = useState({ x: -200, y: -100 });
  const [dragging, setDragging] = useState(false);
  // Ref (not state) so a pan-constraints clamp / wrap adjustment inside a
  // mousemove can synchronously rebase the drag origin — state updates are
  // async and would leave the next move computing delta off a stale start.
  const dragStartRef = useRef({ x: 0, y: 0 });
  const [localScale, setLocalScale] = useState(0.7);
  const [dropupOpen, setDropupOpen] = useState(false);
  const [blinkOn, setBlinkOn] = useState(true);
  const blinkIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const lastTouchRef = useRef<{ x: number; y: number } | null>(null);
  const lastPinchDistRef = useRef<number | null>(null);
  const scaleRef = useRef(scaleProp ?? 0.7);
  const offsetRef = useRef(offsetProp ?? { x: -200, y: -100 });

  // RAF-based render scheduling: gestures (wheel, drag, touch) mutate
  // scaleRef/offsetRef synchronously and call scheduleFrameRef.current(),
  // which coalesces to at most one render per animation frame. State is
  // committed back to props at gesture end (or after a short idle for
  // wheel), so parents stay authoritative for persistence without
  // triggering a React commit on every input event.
  //
  // scheduleFrameRef is assigned below (once render + scheduleFrame are
  // declared) so handlers declared earlier in the component body can still
  // reach the live scheduler without TDZ issues.
  const rafIdRef = useRef<number | null>(null);
  const gestureCommitTimerRef = useRef<number | null>(null);
  const scheduleFrameRef = useRef<() => void>(() => {});

  const offset = offsetProp ?? localOffset;
  const scale = scaleProp ?? localScale;
  useLayoutEffect(() => { scaleRef.current = scale; offsetRef.current = offset; }, [scale, offset]);
  const setOffset = onOffsetChange ?? setLocalOffset;
  const setScale = (valOrFn: number | ((prev: number) => number)) => {
    if (onScaleChange) {
      const newVal = typeof valOrFn === 'function' ? valOrFn(scale) : valOrFn;
      onScaleChange(newVal);
    } else {
      setLocalScale(valOrFn as any);
    }
  };

  const cancelCommitTimer = () => {
    if (gestureCommitTimerRef.current != null) {
      window.clearTimeout(gestureCommitTimerRef.current);
      gestureCommitTimerRef.current = null;
    }
  };
  const commitGestureStateNow = () => {
    cancelCommitTimer();
    setOffset(offsetRef.current);
    setScale(scaleRef.current);
  };
  const scheduleGestureCommit = (delayMs = 150) => {
    cancelCommitTimer();
    gestureCommitTimerRef.current = window.setTimeout(() => {
      gestureCommitTimerRef.current = null;
      setOffset(offsetRef.current);
      setScale(scaleRef.current);
    }, delayMs);
  };

  // ── Hex tooltip state ──────────────────────────────────────
  // Timer refs drive the 1 s open / +1.5 s pin thresholds.
  interface TooltipState {
    kind: 'tile' | 'marker';
    tile?: TileData;
    marker?: NavyMarker;
    hexQ: number;
    hexR: number;
    screenX: number;
    screenY: number;
    sticky: boolean;
  }
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const hoverKeyRef = useRef<string | null>(null);
  const hoverTargetRef = useRef<{ tile?: TileData; marker?: NavyMarker; hexQ: number; hexR: number } | null>(null);
  const hoverPosRef = useRef<{ x: number; y: number } | null>(null);
  const openTimerRef = useRef<number | null>(null);
  const pinTimerRef = useRef<number | null>(null);

  const clearTooltipTimers = useCallback(() => {
    if (openTimerRef.current != null) {
      window.clearTimeout(openTimerRef.current);
      openTimerRef.current = null;
    }
    if (pinTimerRef.current != null) {
      window.clearTimeout(pinTimerRef.current);
      pinTimerRef.current = null;
    }
  }, []);

  const closeNonStickyTooltip = useCallback(() => {
    clearTooltipTimers();
    hoverKeyRef.current = null;
    hoverTargetRef.current = null;
    setTooltip(prev => (prev && prev.sticky ? prev : null));
  }, [clearTooltipTimers]);

  const armTooltipTimer = useCallback((token: string) => {
    clearTooltipTimers();
    openTimerRef.current = window.setTimeout(() => {
      // Validate that the hover target the timer was scheduled for is still
      // what the cursor is over; discard stale callbacks.
      if (hoverKeyRef.current !== token) return;
      const tgt = hoverTargetRef.current;
      const pos = hoverPosRef.current;
      if (!tgt || !pos) return;
      setTooltip(prev => {
        // A sticky tooltip must never be replaced by a hover-opened one.
        if (prev?.sticky) return prev;
        return {
          kind: tgt.tile ? 'tile' : 'marker',
          tile: tgt.tile,
          marker: tgt.marker,
          hexQ: tgt.hexQ,
          hexR: tgt.hexR,
          screenX: pos.x,
          screenY: pos.y,
          sticky: false,
        };
      });
      pinTimerRef.current = window.setTimeout(() => {
        if (hoverKeyRef.current !== token) return;
        setTooltip(prev => (prev ? { ...prev, sticky: true } : null));
      }, 1500);
    }, 1000);
  }, [clearTooltipTimers]);

  useEffect(() => () => clearTooltipTimers(), [clearTooltipTimers]);

  // Canvas viewport size — updated by the ResizeObserver below so the
  // off-screen dismissal effect fires on resize too (not just pan/zoom).
  const [canvasSize, setCanvasSize] = useState({ w: 0, h: 0 });

  // Map pixel dimensions used for pan clamping, min-zoom clamp, and the
  // horizontal-wrap renderer. Every tile carries the same map_width /
  // map_height from the bridge, so derive from tiles[0].
  //   mapPixelWidth  = wrap period in world x (pointy-top: every row shifts
  //                    by mapWidth*HEX_SIZE*SQRT3 when q advances by mapWidth)
  //   minWorldY / mapPixelHeight = total vertical extent including the half-
  //                    hex caps above r=0 and below r=mapHeight-1, so the
  //                    clamp pins the north/south edge of the hex silhouette
  //                    (not just the tile centers) to the viewport.
  const mapDims = useMemo(() => {
    if (tiles.length === 0) {
      return { mapWidth: 0, mapHeight: 0, mapPixelWidth: 0, mapPixelHeight: 0, minWorldY: 0 };
    }
    const { map_width: mapWidth, map_height: mapHeight } = tiles[0];
    const mapPixelWidth = mapWidth * HEX_SIZE * SQRT3;
    const minWorldY = -HEX_SIZE;
    const maxWorldY = HEX_SIZE * (1.5 * mapHeight - 0.5);
    const mapPixelHeight = maxWorldY - minWorldY;
    return { mapWidth, mapHeight, mapPixelWidth, mapPixelHeight, minWorldY };
  }, [tiles]);

  /** Apply globe-style pan constraints: vertical clamp at the north/south
   *  map edges, horizontal canonicalization so pan.x stays within one wrap
   *  period of the origin. Used from every call site that writes offsetRef.
   *  Callers that own drag state are responsible for rebasing dragStartRef
   *  against the returned offset to preserve the drag invariant. */
  const applyPanConstraints = useCallback((
    off: { x: number; y: number },
    scl: number,
  ): { x: number; y: number } => {
    const { mapPixelWidth, mapPixelHeight, minWorldY } = mapDims;
    const canvasH = canvasSize.h;
    if (mapPixelHeight === 0 || canvasH === 0) return off;
    const maxY = -minWorldY * scl;
    const minY = canvasH - (minWorldY + mapPixelHeight) * scl;
    let y: number;
    if (minY >= maxY) {
      y = (maxY + minY) / 2;
    } else {
      y = Math.min(maxY, Math.max(minY, off.y));
    }
    let x = off.x;
    if (mapPixelWidth > 0) {
      const periodScreen = mapPixelWidth * scl;
      if (periodScreen > 0) {
        let wrapped = x % periodScreen;
        if (wrapped < -periodScreen / 2) wrapped += periodScreen;
        else if (wrapped >= periodScreen / 2) wrapped -= periodScreen;
        x = wrapped;
      }
    }
    return { x, y };
  }, [mapDims, canvasSize]);

  /** Wrap a hex coord returned by pixelToHex back into the primary map copy
   *  so clicks / hovers on a wrap copy resolve to the correct underlying
   *  tile. No-op until map dims are known. */
  const wrapHex = useCallback((q: number, r: number): [number, number] => {
    const w = mapDims.mapWidth;
    if (w <= 0) return [q, r];
    // Wrap in offset-q space — the world is an offset rectangle, not an
    // axial parallelogram, so naive q-modulo lands outside the row's range.
    const shift = Math.floor(r / 2);
    const qoff = q + shift;
    const wqoff = ((qoff % w) + w) % w;
    return [wqoff - shift, r];
  }, [mapDims]);

  /** Wrap a world-space x coordinate into the primary map copy. Used for
   *  hit-testing anything that lives at world-space positions (navy markers)
   *  so clicks in a wrap copy map to the same underlying entity. */
  const wrapWorldX = useCallback((x: number): number => {
    const period = mapDims.mapPixelWidth;
    if (period <= 0) return x;
    return ((x % period) + period) % period;
  }, [mapDims]);

  // When map dims first become valid (tiles loaded) or the canvas resizes,
  // retroactively snap scale/offset into the clamp range. Without this, a
  // user who zoomed out while tiles were still loading ends up with scale
  // below the height-fit minimum — vertical clamp would then leave black
  // bars above/below the map, and the wrap renderer would be asked to draw
  // dozens of horizontal copies per frame.
  useEffect(() => {
    const { mapPixelHeight } = mapDims;
    const canvasH = canvasSize.h;
    if (mapPixelHeight === 0 || canvasH === 0) return;
    const fitScale = canvasH / mapPixelHeight;
    let nextScale = scaleRef.current;
    if (nextScale < fitScale) nextScale = fitScale;
    const maxScale = lockZoom ? fitScale : 4;
    if (nextScale > maxScale) nextScale = maxScale;
    const nextOffset = applyPanConstraints(offsetRef.current, nextScale);
    const changedScale = nextScale !== scaleRef.current;
    const changedOffset = nextOffset.x !== offsetRef.current.x || nextOffset.y !== offsetRef.current.y;
    if (!changedScale && !changedOffset) return;
    scaleRef.current = nextScale;
    offsetRef.current = nextOffset;
    if (changedScale) setScale(nextScale);
    if (changedOffset) setOffset(nextOffset);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapDims, canvasSize, applyPanConstraints, lockZoom]);

  // If a pinned tooltip's hex has been panned/zoomed (or the canvas resized)
  // off-screen, dismiss it. With globe-style wrap, the hex may also be
  // on-screen via a wrap copy, so check vertical only for dismissal and
  // confirm horizontal against the nearest wrap copy.
  useEffect(() => {
    if (!tooltip) return;
    const cw = canvasSize.w;
    const ch = canvasSize.h;
    if (cw === 0 || ch === 0) return;
    const [px, py] = hexToPixel(tooltip.hexQ, tooltip.hexR);
    const sy = py * scale + offset.y;
    const pad = HEX_SIZE * scale;
    if (sy < -pad || sy > ch + pad) {
      setTooltip(null);
      return;
    }
    const periodScreen = mapDims.mapPixelWidth * scale;
    let sxPrimary = px * scale + offset.x;
    if (periodScreen > 0) {
      // Shift sxPrimary into the same wrap copy that's closest to the viewport.
      const viewportCenter = cw / 2;
      const offsetFromCenter = sxPrimary - viewportCenter;
      const shifts = Math.round(offsetFromCenter / periodScreen);
      sxPrimary -= shifts * periodScreen;
    }
    if (sxPrimary < -pad || sxPrimary > cw + pad) {
      setTooltip(null);
    }
  }, [tooltip, scale, offset, canvasSize, mapDims]);

  /** Pure zoom-to-point math: returns clamped new scale and adjusted offset.
   *  Min-zoom is dynamic — clamped so the map's vertical extent fully fills
   *  the canvas (no sky/sea bars above or below). Horizontally the world is
   *  treated as a globe: the wrap renderer always tiles copies across the
   *  canvas, so any horizontal "gap" beyond one map width is filled by the
   *  wrap copies regardless of where the user is panned. */
  const computeZoom = (cx: number, cy: number, oldScale: number, oldOffset: { x: number; y: number }, newScale: number) => {
    const { mapPixelHeight } = mapDims;
    const canvasH = canvasSize.h;
    const fitScale = (mapPixelHeight > 0 && canvasH > 0)
      ? (canvasH / mapPixelHeight)
      : 0.1;
    const maxScale = lockZoom ? fitScale : 4;
    const clamped = Math.max(fitScale, Math.min(maxScale, newScale));
    const ratio = clamped / oldScale;
    const rawOffset = { x: cx - (cx - oldOffset.x) * ratio, y: cy - (cy - oldOffset.y) * ratio };
    return { scale: clamped, offset: applyPanConstraints(rawOffset, clamped) };
  };

  /** Zoom toward a screen-space point, adjusting offset so that point stays fixed.
   *  Updates refs + schedules a frame synchronously; commit to React state is
   *  debounced so wheel bursts don't re-render App per event. */
  const zoomAt = (cx: number, cy: number, newScale: number) => {
    const z = computeZoom(cx, cy, scaleRef.current, offsetRef.current, newScale);
    scaleRef.current = z.scale;
    offsetRef.current = z.offset;
    scheduleFrameRef.current();
    scheduleGestureCommit();
  };
  // Mirror zoomAt / computeZoom into refs so listeners that are bound once
  // via useEffect with a narrow dep list (native wheel, keydown) always invoke
  // the freshest closure — otherwise computeZoom uses a stale mapDims /
  // canvasSize after a window resize and the zoom-out clamp ends up too high.
  const zoomAtRef = useRef(zoomAt);
  zoomAtRef.current = zoomAt;
  const computeZoomRef = useRef(computeZoom);
  computeZoomRef.current = computeZoom;

  // When lockZoom becomes true, snap to fitScale immediately so the map fills the canvas.
  const prevLockZoomRef = useRef(lockZoom);
  useEffect(() => {
    if (lockZoom && !prevLockZoomRef.current) {
      const canvas = canvasRef.current;
      const cx = canvas ? canvas.clientWidth / 2 : 0;
      const cy = canvas ? canvas.clientHeight / 2 : 0;
      const z = computeZoomRef.current(cx, cy, scaleRef.current, offsetRef.current, 0.01);
      scaleRef.current = z.scale;
      offsetRef.current = z.offset;
      setScale(z.scale);
      setOffset(z.offset);
      scheduleFrameRef.current();
    }
    prevLockZoomRef.current = lockZoom;
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lockZoom]);

  const showPoliticalColors = mapMode !== 'terrain';

  const tileMap = useMemo(() => {
    const m = new Map<string, TileData>();
    for (const tile of tiles) {
      m.set(`${tile.q},${tile.r}`, tile);
    }
    return m;
  }, [tiles]);

  // Per-tile world-space center, computed once per tiles change. The render
  // path uses it for frustum culling instead of paying hexToPixel per tile per
  // pass.
  const tilePositions = useMemo(() => {
    const out = new Array<{ tile: TileData; px: number; py: number }>(tiles.length);
    for (let i = 0; i < tiles.length; i++) {
      const t = tiles[i];
      const [px, py] = hexToPixel(t.q, t.r);
      out[i] = { tile: t, px, py };
    }
    return out;
  }, [tiles]);

  // Border-relevant fingerprint. mapGeometry / classifiedEdges only need to
  // recompute when a tile's coastline (terrain Sea?), country (visual_group /
  // owner) or province assignment changes. Most turn-tick fields (visible,
  // civilian_on_tile, army_*, etc.) are border-irrelevant. Pinning the heavy
  // border memos to this fingerprint lets ownership-stable turns reuse the
  // previous result.
  const borderSignature = useMemo(() => {
    const end = perfMark(`borderSignature (${tiles.length} tiles)`);
    const parts = new Array<string>(tiles.length + 1);
    parts[0] = String(tiles[0]?.map_width ?? 0);
    for (let i = 0; i < tiles.length; i++) {
      const t = tiles[i];
      parts[i + 1] = `${t.q},${t.r},${t.terrain === 'Sea' ? 1 : 0},${t.visual_group ?? ''},${t.owner ?? ''},${t.province ?? ''}`;
    }
    const sig = parts.join('|');
    end();
    return sig;
  }, [tiles]);
  const tilesRef = useRef(tiles);
  tilesRef.current = tiles;

  // Bake-relevant per-tile fields not covered by borderSignature: full
  // terrain subtype (Forest vs Desert), owner_color, is_incorporated_minor.
  // These all affect tileFillColor() output, so we must invalidate the
  // static cache when any of them change. They live in a separate signature
  // so border-only memos (mapGeometry / classifiedEdges) stay independent.
  const fillSignature = useMemo(() => {
    const bits = new Array<string>(tiles.length);
    for (let i = 0; i < tiles.length; i++) {
      const t = tiles[i];
      bits[i] = `${t.terrain},${t.owner_color ?? ''},${t.is_incorporated_minor ? 1 : 0}`;
    }
    return bits.join('|');
  }, [tiles]);

  const SEA_ZONE_FILL_COLOR = 'rgba(20, 70, 130, 0.12)';
  const SEA_ZONE_BORDER_COLOR = 'rgba(0, 40, 100, 0.45)';

  // Build nation → overlay fill color map for diplomatic/relationship/military/naval modes
  const nationFillMap = useMemo(() => {
    const m = new Map<string, string>();
    if (mapMode === 'diplomatic' && diplomacyOverlay) {
      m.set(diplomacyOverlay.selected_nation, '#ffd900');
      for (const rel of diplomacyOverlay.relations) {
        m.set(rel.nation_name, DIPLO_STATUS_COLORS[rel.status] || '#666666');
      }
    } else if (mapMode === 'relationship' && diplomacyOverlay) {
      m.set(diplomacyOverlay.selected_nation, '#ffd900');
      for (const rel of diplomacyOverlay.relations) {
        m.set(rel.nation_name, scoreToColor(rel.score));
      }
    } else if (mapMode === 'military' && militaryOverlay) {
      const values = militaryOverlay.map(e => e.total_army_fp);
      const avg = values.reduce((a, b) => a + b, 0) / Math.max(1, values.length);
      const maxDev = Math.max(1, ...values.map(v => Math.abs(v - avg)));
      for (const entry of militaryOverlay) {
        // Score: -100 (weakest) → 0 (average) → +100 (strongest)
        const score = Math.round(((entry.total_army_fp - avg) / maxDev) * 100);
        m.set(entry.nation_name, strengthToColor(score));
      }
    } else if (mapMode === 'naval' && militaryOverlay) {
      const values = militaryOverlay.map(e => e.total_naval_fp);
      const avg = values.reduce((a, b) => a + b, 0) / Math.max(1, values.length);
      const maxDev = Math.max(1, ...values.map(v => Math.abs(v - avg)));
      for (const entry of militaryOverlay) {
        const score = Math.round(((entry.total_naval_fp - avg) / maxDev) * 100);
        m.set(entry.nation_name, strengthToColor(score));
      }
    }
    return m;
  }, [mapMode, diplomacyOverlay, militaryOverlay]);
  const nationFillMapRef = useRef(nationFillMap);
  nationFillMapRef.current = nationFillMap;

  // Content signature for nationFillMap. The Map's reference churns on every
  // wasm poll because diplomacyOverlay/militaryOverlay are re-fetched each
  // poll (new prop refs even when content is identical). Pinning the static
  // bake to a content-string instead of the Map ref means polls without
  // overlay-content changes don't invalidate the cache.
  const nationFillSignature = useMemo(() => {
    const entries: string[] = [];
    for (const [k, v] of nationFillMap) entries.push(`${k}=${v}`);
    entries.sort();
    return entries.join(',');
  }, [nationFillMap]);

  // Memoize nation label BFS — only recompute when tiles change, not on pan/zoom
  // Uses visual_group so incorporated minor nations get their own label
  const nationLabels = useMemo(() => computeNationLabels(tiles), [tiles]);

  // Province label centroids — used for zoom-in province name rendering
  const provinceLabels = useMemo(() => {
    const map = new Map<string, { cx: number; cy: number; size: number }>();
    for (const tile of tiles) {
      if (tile.terrain === 'Sea' || !tile.province) continue;
      const [px, py] = [HEX_SIZE * (Math.sqrt(3) * tile.q + Math.sqrt(3) / 2 * tile.r),
                        HEX_SIZE * (3 / 2 * tile.r)];
      const entry = map.get(tile.province);
      if (entry) {
        entry.cx = (entry.cx * entry.size + px) / (entry.size + 1);
        entry.cy = (entry.cy * entry.size + py) / (entry.size + 1);
        entry.size += 1;
      } else {
        map.set(tile.province, { cx: px, cy: py, size: 1 });
      }
    }
    return Array.from(map.entries()).map(([name, v]) => ({ name, ...v }));
  }, [tiles]);

  // ── Organic coastline + border geometry ─────────────────────────────────
  //
  // Builds smoothed polylines for the land/sea boundary and for political
  // borders so the map doesn't look visibly hexagonal at its silhouette. The
  // hex grid is untouched for gameplay; this is pure presentation. See
  // web/src/lib/mapGeometry.ts. Computed only when organicBorders is on.
  const mapGeometry = useMemo(() => {
    if (!organicBorders) return null;
    const tiles = tilesRef.current;
    const end = perfMark(`mapGeometry (${tiles.length} tiles)`);
    const verts = hexVertices(HEX_SIZE);
    const tileMapLocal = new Map<string, TileData>();
    for (const tile of tiles) tileMapLocal.set(`${tile.q},${tile.r}`, tile);
    // Wrap-aware neighbor lookup: the world is a globe, so when a neighbor's
    // q is out of [0, mapWidth), it wraps around — (q=-1) lives in the wrap
    // copy at (q=mapWidth-1). Without this, tiles on the east / west map
    // seam are treated as coastlines and their component clip paths close
    // off with a vertical seam, so land / country color gets clipped at an
    // unnatural straight line where the seam sits. With it, adjacent tiles
    // across the seam are correctly identified as interior neighbours and
    // the border / coast generation skips the seam entirely.
    const mw = tiles[0]?.map_width ?? 0;
    // Wrap in offset-q space (q + floor(r/2)) rather than raw axial q. The
    // world is an offset rectangle: each row r holds q in
    // [-floor(r/2), mw - floor(r/2)), so naive q-modulo would wrap into
    // coordinates that don't exist for that row. Wrapping in offset-q first,
    // then converting back, lands on the actual stored neighbor across the
    // seam.
    const wrapNeighbor = (nq: number, nr: number): TileData | undefined => {
      if (mw <= 0) return tileMapLocal.get(`${nq},${nr}`);
      const shift = Math.floor(nr / 2);
      const qoff = nq + shift;
      const wqoff = ((qoff % mw) + mw) % mw;
      const wq = wqoff - shift;
      return tileMapLocal.get(`${wq},${nr}`);
    };

    // Collect coastline hex edges (land facing sea or map edge) plus political
    // edges (province and country-interior), and a canonical outward normal
    // per edge. The normal is looked up — not recomputed from walk direction —
    // when displacing a polyline, so every visitor of the edge (standalone
    // stroke, per-component clip on one side, per-component clip on the
    // other side) agrees on the same displaced curve.
    const vertexCoord = new Map<string, Vec2>();
    const coastEdges: { a: string; b: string }[] = [];
    const provinceEdgeList: { a: string; b: string }[] = [];
    const countryInteriorEdgeList: { a: string; b: string }[] = [];
    const edgeNormal = new Map<string, Vec2>();
    const edgeAmpMult = new Map<string, number>();
    const seenPoliticalEdges = new Set<string>();
    const landHexes: TileData[] = [];

    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;

    const storeVertex = (k: string, x: number, y: number) => {
      if (!vertexCoord.has(k)) vertexCoord.set(k, [x, y]);
    };
    const politicalKey = (k1: string, k2: string) => (k1 < k2 ? `${k1}|${k2}` : `${k2}|${k1}`);

    for (const tile of tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      // Bbox: include every tile so the sea-fill rect covers the map.
      for (const [vx, vy] of verts) {
        const x = px + vx, y = py + vy;
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
      if (tile.terrain === 'Sea') continue;
      landHexes.push(tile);
      const neighbors = hexNeighbors(tile.q, tile.r);
      const tileVG = tile.visual_group || tile.owner;
      for (let i = 0; i < 6; i++) {
        const [nq, nr] = neighbors[i];
        const neighbor = wrapNeighbor(nq, nr);
        const v1 = verts[i];
        const v2 = verts[(i + 1) % 6];
        const x1 = px + v1[0], y1 = py + v1[1];
        const x2 = px + v2[0], y2 = py + v2[1];
        const k1 = vKey(x1, y1);
        const k2 = vKey(x2, y2);
        storeVertex(k1, x1, y1);
        storeVertex(k2, x2, y2);

        const isCoast = !neighbor || neighbor.terrain === 'Sea';
        const neighborVG = neighbor ? (neighbor.visual_group || neighbor.owner) : '';
        const isCountry = !isCoast && tileVG !== neighborVG;
        const isProvince = !isCoast && !isCountry && !!tile.owner && tile.province !== neighbor!.province;
        if (!isCoast && !isCountry && !isProvince) continue; // interior edge: stays straight

        const pk = politicalKey(k1, k2);

        // Canonical outward normal, computed ONCE per edge. For coast edges
        // "outward" means from the land hex toward the sea side. For land-to-
        // land edges (country/province) there's no physical "outward" so we
        // pick a deterministic direction: perpendicular CCW of the sorted
        // a→b vector. Any single fixed choice works as long as every visitor
        // uses the same.
        if (!edgeNormal.has(pk)) {
          // Perpendicular to the segment (sorted a→b for determinism).
          const [sa, sb] = k1 < k2 ? [k1, k2] : [k2, k1];
          const pa = vertexCoord.get(sa)!;
          const pb = vertexCoord.get(sb)!;
          const dx = pb[0] - pa[0], dy = pb[1] - pa[1];
          const len = Math.hypot(dx, dy) || 1;
          let nx = -dy / len, ny = dx / len;
          const mx = (pa[0] + pb[0]) * 0.5;
          const my = (pa[1] + pb[1]) * 0.5;
          if (isCoast) {
            // Flip to point away from the land hex.
            if ((mx - px) * nx + (my - py) * ny < 0) { nx = -nx; ny = -ny; }
          }
          edgeNormal.set(pk, [nx, ny]);

          // Ruggedness multiplier: smoothly-varying per-midpoint noise,
          // remapped from [-1, 1] into [RUGGEDNESS_MIN, RUGGEDNESS_MAX].
          const rRaw = fbm(mx * RUGGEDNESS_FREQUENCY, my * RUGGEDNESS_FREQUENCY, RUGGEDNESS_OCTAVES, RUGGEDNESS_SEED);
          const t = Math.max(0, Math.min(1, (rRaw + 1) * 0.5));
          edgeAmpMult.set(pk, RUGGEDNESS_MIN + (RUGGEDNESS_MAX - RUGGEDNESS_MIN) * t);
        }

        if (isCoast) {
          coastEdges.push({ a: k1, b: k2 });
          continue;
        }
        if (isCountry) {
          if (!seenPoliticalEdges.has(pk)) {
            seenPoliticalEdges.add(pk);
            countryInteriorEdgeList.push({ a: k1, b: k2 });
          }
        } else if (isProvince) {
          if (!seenPoliticalEdges.has(pk)) {
            seenPoliticalEdges.add(pk);
            provinceEdgeList.push({ a: k1, b: k2 });
          }
        }
      }
    }

    // Build a per-segment normal array for an ordered key sequence, using the
    // canonical normals that were computed during edge collection.
    const normalsFor = (keys: string[], closed: boolean): Vec2[] => {
      const n = keys.length;
      const segCount = closed ? n : n - 1;
      const out: Vec2[] = new Array(segCount);
      for (let i = 0; i < segCount; i++) {
        const pk = politicalKey(keys[i], keys[(i + 1) % n]);
        out[i] = edgeNormal.get(pk) ?? [0, 0];
      }
      return out;
    };

    const ampMultsFor = (keys: string[], closed: boolean): number[] => {
      const n = keys.length;
      const segCount = closed ? n : n - 1;
      const out = new Array<number>(segCount);
      for (let i = 0; i < segCount; i++) {
        const pk = politicalKey(keys[i], keys[(i + 1) % n]);
        out[i] = edgeAmpMult.get(pk) ?? 1;
      }
      return out;
    };

    // All border buckets use the same noise field (frequency / seed / octaves)
    // so where different classes of border pass through the same point, they
    // displace by correlated amounts and read as drawn on a single map.
    // smoothPolylineAnchored applies Chaikin per-segment with hex vertices
    // pinned — this is what makes two neighbouring nations' clip boundaries
    // agree exactly along their shared edges (no blue gap).
    const smoothKeys = (keys: string[], amplitude: number, subdiv: number, closed: boolean): Vec2[] => {
      const pts = keys.map(k => vertexCoord.get(k)!) as Vec2[];
      if (pts.length < 2) return [];
      const segCount = closed ? pts.length : pts.length - 1;
      const mults = ampMultsFor(keys, closed);
      const segAmp = new Array<number>(segCount);
      for (let i = 0; i < segCount; i++) segAmp[i] = amplitude * mults[i];
      const segSub = new Array<number>(segCount).fill(subdiv);
      const segNormals = normalsFor(keys, closed);
      return smoothPolylineAnchored(pts, segAmp, segSub, {
        frequency: BORDER_FREQUENCY,
        octaves: BORDER_OCTAVES,
        seed: BORDER_SEED,
        smoothing: BORDER_SMOOTHING,
        closed,
        segNormals,
      });
    };

    const smoothBucket = (
      edgeList: { a: string; b: string }[],
      amplitude: number,
      subdiv: number,
    ): { closed: Vec2[][]; open: Vec2[][]; openKeys: string[][] } => {
      const { closed: closedKeys, open: openKeysRaw } = stitchPolylines(edgeList);
      const closedPaths: Vec2[][] = [];
      for (const keys of closedKeys) {
        if (keys.length < 3) continue;
        const smoothed = smoothKeys(keys, amplitude, subdiv, true);
        if (smoothed.length >= 3) closedPaths.push(smoothed);
      }
      const openPaths: Vec2[][] = [];
      for (const keys of openKeysRaw) {
        if (keys.length < 2) continue;
        const smoothed = smoothKeys(keys, amplitude, subdiv, false);
        if (smoothed.length >= 2) openPaths.push(smoothed);
      }
      return { closed: closedPaths, open: openPaths, openKeys: openKeysRaw };
    };

    const coastBucket = smoothBucket(coastEdges, COAST_AMPLITUDE, COAST_SUBDIV);
    const smoothedClosed = coastBucket.closed;
    const smoothedOpen = coastBucket.open;
    const openKeys = coastBucket.openKeys;

    // ── Per-visual-group connected components for anti-spill clipping ──
    // Group land hexes into connected regions that share the same visual_group
    // (owner or incorporated-minor parent). For each region, stitch its
    // boundary — which alternates between coast and country-interior segments
    // — into closed polygons, then smooth with PER-SEGMENT amplitudes that
    // match the separately-drawn strokes. Clipping fills to this polygon
    // prevents a nation's color from leaking past the smoothed border.
    const tileComp = new Map<string, number>();
    const compVg: string[] = [];
    // Components store stable `${q},${r}` keys instead of TileData references
    // so render can re-resolve to the *current* TileData via tileMap each
    // frame. The mapGeometry memo is keyed by borderSignature (q/r/Sea?/
    // visual_group/owner/province) — non-border fields like terrain subtype
    // (Forest vs Desert), is_incorporated_minor, and nation_id can change
    // without triggering recompute, so the fill/highlight passes that read
    // those fields must see the live tile, not a snapshot.
    const compTileKeys: string[][] = [];
    for (const tile of landHexes) {
      const tk = `${tile.q},${tile.r}`;
      if (tileComp.has(tk)) continue;
      const vg = tile.visual_group || tile.owner || '';
      const idx = compTileKeys.length;
      const queue: TileData[] = [tile];
      const members: string[] = [];
      tileComp.set(tk, idx);
      while (queue.length > 0) {
        const t = queue.shift()!;
        members.push(`${t.q},${t.r}`);
        for (const [nq, nr] of hexNeighbors(t.q, t.r)) {
          const n = wrapNeighbor(nq, nr);
          if (!n || n.terrain === 'Sea') continue;
          const nvg = n.visual_group || n.owner || '';
          if (nvg !== vg) continue;
          const ntk = `${n.q},${n.r}`;
          if (tileComp.has(ntk)) continue;
          tileComp.set(ntk, idx);
          queue.push(n);
        }
      }
      compVg.push(vg);
      compTileKeys.push(members);
    }

    // Collect each component's boundary edges with type (coast vs country).
    type BoundaryEdge = { a: string; b: string; type: 'coast' | 'country' };
    const compBoundary: BoundaryEdge[][] = compVg.map(() => []);
    for (const tile of landHexes) {
      const compIdx = tileComp.get(`${tile.q},${tile.r}`)!;
      const [px, py] = hexToPixel(tile.q, tile.r);
      const neighbors = hexNeighbors(tile.q, tile.r);
      const tileVG = tile.visual_group || tile.owner || '';
      for (let i = 0; i < 6; i++) {
        const [nq, nr] = neighbors[i];
        const n = wrapNeighbor(nq, nr);
        let type: 'coast' | 'country' | null = null;
        if (!n || n.terrain === 'Sea') type = 'coast';
        else {
          const nvg = n.visual_group || n.owner || '';
          if (nvg !== tileVG) type = 'country';
        }
        if (type == null) continue;
        const v1 = verts[i];
        const v2 = verts[(i + 1) % 6];
        const x1 = px + v1[0], y1 = py + v1[1];
        const x2 = px + v2[0], y2 = py + v2[1];
        const k1 = vKey(x1, y1);
        const k2 = vKey(x2, y2);
        compBoundary[compIdx].push({ a: k1, b: k2, type });
      }
    }

    // Build one smoothed, closed Path2D per component. Each segment uses the
    // amplitude of its type so the polygon coincides with the strokes that
    // are drawn as coast / country-border polylines elsewhere — and the
    // canonical per-edge normals make shared edges displace identically on
    // both sides, so two neighbouring nations' clips kiss exactly along the
    // border with no gap and no overlap.
    // memberKeys is bundled into each entry so render passes that pair a clip
    // path with its component members iterate the same array — no parallel-
    // index assumption between componentClips and compTileKeys (a degenerate
    // component with an empty boundary would skew that alignment).
    const componentClips: Array<{ path: Path2D; tileKeys: Set<string>; memberKeys: string[] }> = [];
    for (let idx = 0; idx < compVg.length; idx++) {
      const boundary = compBoundary[idx];
      if (boundary.length === 0) continue;
      const typeMap = new Map<string, 'coast' | 'country'>();
      for (const { a, b, type } of boundary) typeMap.set(politicalKey(a, b), type);
      const { closed: closedLoops, open: openLoops } = stitchPolylines(
        boundary.map(({ a, b }) => ({ a, b })),
      );
      const path = new Path2D();
      const smoothLoop = (keys: string[], isClosed: boolean) => {
        const pts = keys.map(k => vertexCoord.get(k)!) as Vec2[];
        if (pts.length < 2) return null;
        const n = pts.length;
        const segCount = isClosed ? n : n - 1;
        const segAmp = new Array<number>(segCount);
        const segSub = new Array<number>(segCount);
        const mults = ampMultsFor(keys, isClosed);
        for (let i = 0; i < segCount; i++) {
          const t = typeMap.get(politicalKey(keys[i], keys[(i + 1) % n]));
          if (t === 'coast') { segAmp[i] = COAST_AMPLITUDE * mults[i]; segSub[i] = COAST_SUBDIV; }
          else if (t === 'country') { segAmp[i] = COUNTRY_BORDER_AMPLITUDE * mults[i]; segSub[i] = COUNTRY_BORDER_SUBDIV; }
          else { segAmp[i] = 0; segSub[i] = 2; }
        }
        const segNormals = normalsFor(keys, isClosed);
        return smoothPolylineAnchored(pts, segAmp, segSub, {
          frequency: BORDER_FREQUENCY, octaves: BORDER_OCTAVES, seed: BORDER_SEED,
          smoothing: BORDER_SMOOTHING, closed: isClosed, segNormals,
        });
      };
      const appendLoop = (loop: Vec2[]) => {
        path.moveTo(loop[0][0], loop[0][1]);
        for (let i = 1; i < loop.length; i++) path.lineTo(loop[i][0], loop[i][1]);
        path.closePath();
      };
      for (const keys of closedLoops) {
        const loop = smoothLoop(keys, true);
        if (loop && loop.length >= 3) appendLoop(loop);
      }
      for (const keys of openLoops) {
        const loop = smoothLoop(keys, false);
        if (loop && loop.length >= 2) appendLoop(loop);
      }
      const memberKeys = compTileKeys[idx];
      const tileKeys = new Set(memberKeys);
      componentClips.push({ path, tileKeys, memberKeys });
    }

    // Precompute vertex-key → land tile keys so the open-polyline fallback
    // below can look up adjacent land hexes in O(1) instead of scanning.
    const vertexToLandHexes = new Map<string, string[]>();
    for (const tile of landHexes) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      const tk = `${tile.q},${tile.r}`;
      for (const [vx, vy] of verts) {
        const k = vKey(px + vx, py + vy);
        const list = vertexToLandHexes.get(k);
        if (list) list.push(tk); else vertexToLandHexes.set(k, [tk]);
      }
    }

    // Stitched coastlines should almost always close (each coastline vertex
    // has degree 2). When one doesn't — e.g. a degenerate isthmus with a
    // degree-4 vertex — the resulting open polyline can't seal a land
    // region on its own. Track the tiles adjacent to any such vertex so
    // their hex shape can be added to the clip path as a fallback.
    const openFallbackHexKeys = new Set<string>();
    for (const keys of openKeys) {
      for (const k of keys) {
        const hexes = vertexToLandHexes.get(k);
        if (hexes) for (const tk of hexes) openFallbackHexKeys.add(tk);
      }
    }

    // Build the land clip path: smoothed closed coastline polygons (as a
    // union via evenodd), PLUS fallback hex polygons for any open-polyline
    // land tiles so their region is still filled.
    const buildClipPath = (): Path2D => {
      const p = new Path2D();
      for (const loop of smoothedClosed) {
        if (loop.length === 0) continue;
        p.moveTo(loop[0][0], loop[0][1]);
        for (let i = 1; i < loop.length; i++) p.lineTo(loop[i][0], loop[i][1]);
        p.closePath();
      }
      for (const tile of landHexes) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        if (!openFallbackHexKeys.has(`${tile.q},${tile.r}`)) continue;
        p.moveTo(px + verts[0][0], py + verts[0][1]);
        for (let i = 1; i < 6; i++) p.lineTo(px + verts[i][0], py + verts[i][1]);
        p.closePath();
      }
      return p;
    };

    // Country and province borders — same noise field as the coast so they
    // agree where they meet.
    const country = smoothBucket(countryInteriorEdgeList, COUNTRY_BORDER_AMPLITUDE, COUNTRY_BORDER_SUBDIV);
    const province = smoothBucket(provinceEdgeList, PROVINCE_BORDER_AMPLITUDE, PROVINCE_BORDER_SUBDIV);

    // Pad the sea-fill rect so it overshoots any noise-displaced coastline.
    const PAD = HEX_SIZE * 1.2;
    end();
    return {
      seaBox: { x: minX - PAD, y: minY - PAD, w: (maxX - minX) + 2 * PAD, h: (maxY - minY) + 2 * PAD },
      clipPath: buildClipPath(),
      coastPolylinesClosed: smoothedClosed,
      coastPolylinesOpen: smoothedOpen,
      countryPolylinesClosed: country.closed,
      countryPolylinesOpen: country.open,
      provincePolylinesClosed: province.closed,
      provincePolylinesOpen: province.open,
      componentClips,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [borderSignature, organicBorders]);

  /** Pre-classified hex edges (flat [x1,y1,x2,y2,...] arrays). Depends only
   *  on tiles/tileMap — not on scale/offset — so it doesn't recompute during
   *  zoom/pan. In organic-borders mode `country`/`province` go unused (the
   *  smoothed polylines handle those), but they're cheap to produce and the
   *  non-organic fallback in render uses them without a second pass. */
  const classifiedEdges = useMemo(() => {
    const tiles = tilesRef.current;
    const end = perfMark(`classifiedEdges (${tiles.length} tiles)`);
    const verts = hexVertices(HEX_SIZE);
    const normalEdges: number[] = [];
    const provinceEdges: number[] = [];
    const countryEdges: number[] = [];
    // Wrap-aware neighbor lookup: same rationale as mapGeometry — the east /
    // west map edges are wrap seams, not coastlines, so edge classification
    // must follow the wrapped neighbour (else the non-organic fallback also
    // shows a phantom country stroke at the seam).
    const mw = tiles[0]?.map_width ?? 0;
    // Wrap in offset-q space — see mapGeometry's wrapNeighbor for the why.
    const neighborAt = (nq: number, nr: number): TileData | undefined => {
      if (mw <= 0) return tileMap.get(`${nq},${nr}`);
      const shift = Math.floor(nr / 2);
      const qoff = nq + shift;
      const wqoff = ((qoff % mw) + mw) % mw;
      const wq = wqoff - shift;
      return tileMap.get(`${wq},${nr}`);
    };
    for (const tile of tiles) {
      if (tile.terrain === 'Sea') continue;
      const [px, py] = hexToPixel(tile.q, tile.r);
      const neighbors = hexNeighbors(tile.q, tile.r);
      const tileVG = tile.visual_group || tile.owner;
      for (let i = 0; i < 6; i++) {
        const [nq, nr] = neighbors[i];
        const neighbor = neighborAt(nq, nr);
        const neighborVG = neighbor ? (neighbor.visual_group || neighbor.owner) : '';
        const v1 = verts[i];
        const v2 = verts[(i + 1) % 6];
        const x1 = px + v1[0], y1 = py + v1[1];
        const x2 = px + v2[0], y2 = py + v2[1];
        if (!neighbor || neighbor.terrain === 'Sea') {
          if (tile.owner) countryEdges.push(x1, y1, x2, y2);
          continue;
        }
        if (tileVG !== neighborVG) {
          countryEdges.push(x1, y1, x2, y2);
          continue;
        }
        if (tile.owner && tile.province !== neighbor.province) {
          provinceEdges.push(x1, y1, x2, y2);
          continue;
        }
        normalEdges.push(x1, y1, x2, y2);
      }
    }
    end();
    return { normalEdges, provinceEdges, countryEdges };
    // tileMap and tiles read via refs; recompute only when border-relevant
    // fields change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [borderSignature]);

  /** Max army firepower across all capitals — used to normalize the per-capital
   *  strength-bar width. Previously re-scanned every frame. */
  const maxArmyFP = useMemo(() => {
    let m = 0;
    for (const tile of tiles) {
      if (tile.is_capital && tile.army_firepower > m) m = tile.army_firepower;
    }
    return m < 1 ? 1 : m;
  }, [tiles]);

  // ── Static-layer cache (terrain fills + sea zones + borders) ─────────────
  // The heaviest passes (Pass 1 land fills, Pass 1.5 sea zones, Pass 2 hex
  // grid + smoothed border strokes) only depend on tile content, map mode,
  // and the rendering scale. Bake them into an offscreen canvas keyed by the
  // bucketed scale; per-frame work then becomes a single drawImage per wrap
  // copy plus the cheap dynamic overlays. The cache invalidates when:
  //   - tile content relevant to borders or fills changes (borderSignature,
  //     mapMode, nationFillMap)
  //   - sea zones change
  //   - the geometry memos themselves change (mapGeometry, classifiedEdges)
  //   - the scale bucket changes — see staticScaleBucket below
  //   - the user toggles hideHexGrid / organicBorders / showPoliticalColors
  // Within a bucket, smooth pan/zoom never invalidates → cost dominates the
  // *first* frame after a state change, with subsequent frames being a blit.
  // Only-grow bucketing: bake once at the zoom-in resolution and never
  // re-bake on zoom out (drawImage downsampling is fast and crisp). The
  // bucket grows when the user zooms past it, capped at a memory-safe max.
  // A new map (tiles ref change) resets the bucket to fit current scale.
  const computeBucket = (s: number) => Math.min(2.5, Math.max(0.5, Math.round(s * 2) / 2));
  const [staticScaleBucket, setStaticScaleBucket] = useState(() => computeBucket(scale));
  // Note: deliberately do NOT reset the bucket on tiles change — `tiles`
  // ref churns on every wasm poll (even when content is identical), and
  // resetting the bucket per-poll triggers a fresh re-bake. The cap at 2.5
  // is enough to keep memory bounded across game sessions.
  useEffect(() => {
    const desired = computeBucket(scale);
    if (desired > staticScaleBucket) setStaticScaleBucket(desired);
  }, [scale, staticScaleBucket]);

  // Fog of war is part of the static layer (visibility is stable within a
  // turn). Encoding `tile.visible` as a per-tile bit string lets the static
  // cache invalidate exactly when fog flips, without dragging in unrelated
  // tile fields.
  const fogSignature = useMemo(() => {
    if (disableFogOfWar) return 'off';
    const bits = new Array<string>(tiles.length);
    for (let i = 0; i < tiles.length; i++) bits[i] = tiles[i].visible ? '1' : '0';
    return bits.join('');
  }, [tiles, disableFogOfWar]);

  // Sea zones come from the wasm bridge as a fresh array on every poll, even
  // when content is identical. Pinning the static bake to a content sig
  // keeps the cache valid across polls that don't change zone membership.
  const seaZonesSignature = useMemo(() => {
    if (seaZones.length === 0) return '';
    const parts = new Array<string>(seaZones.length);
    for (let i = 0; i < seaZones.length; i++) {
      const z = seaZones[i];
      parts[i] = `${z.id}:${z.hexes.length}:${z.center_q},${z.center_r}:${z.name}`;
    }
    return parts.join('|');
  }, [seaZones]);
  const seaZonesRef = useRef(seaZones);
  seaZonesRef.current = seaZones;

  // Refs let the staticLayer memo read these inside its body without listing
  // them as deps — their refs churn per poll but their content is captured
  // by the various *Signature memos.
  const tilePositionsRef = useRef(tilePositions);
  tilePositionsRef.current = tilePositions;
  const tileMapRef = useRef(tileMap);
  tileMapRef.current = tileMap;
  const mapGeometryRef = useRef(mapGeometry);
  mapGeometryRef.current = mapGeometry;
  const classifiedEdgesRef = useRef(classifiedEdges);
  classifiedEdgesRef.current = classifiedEdges;
  const mapDimsRef = useRef(mapDims);
  mapDimsRef.current = mapDims;

  const staticBbox = useMemo(() => {
    const tilePositions = tilePositionsRef.current;
    if (tilePositions.length === 0) {
      return { minX: 0, minY: 0, maxX: 0, maxY: 0 };
    }
    const verts = hexVertices(HEX_SIZE);
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (let i = 0; i < tilePositions.length; i++) {
      const tp = tilePositions[i];
      for (const [vx, vy] of verts) {
        const x = tp.px + vx, y = tp.py + vy;
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
    }
    return { minX, minY, maxX, maxY };
    // borderSignature covers q/r — the only fields the bbox reads through
    // tilePositions. Pinning to it (instead of tilePositions ref) keeps the
    // bbox stable across polls.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [borderSignature]);

  // Tracks which dep changed between bakes. Helpful when the cache appears
  // to invalidate unexpectedly (e.g. during pan, which should never re-bake).
  const lastStaticDepsRef = useRef<Record<string, unknown> | null>(null);
  const staticLayer = useMemo(() => {
    if (PERF_LOG) {
      const cur: Record<string, unknown> = {
        borderSignature, fillSignature, fogSignature,
        seaZonesSignature, nationFillSignature,
        mapMode, hideHexGrid, organicBorders, disableFogOfWar,
        staticBbox, staticScaleBucket, canvasSize_h: canvasSize.h,
      };
      const prev = lastStaticDepsRef.current;
      if (prev) {
        const changed: string[] = [];
        for (const k of Object.keys(cur)) {
          if (cur[k] !== prev[k]) changed.push(k);
        }
        if (changed.length > 0) {
          console.log(`[hexmap] staticLayer rebake — changed deps: ${changed.join(', ')}`);
        }
      }
      lastStaticDepsRef.current = cur;
    }
    // Read all ref-churning values from refs so they don't appear in the
    // dep array. The various *Signature deps below capture content stability.
    const tiles = tilesRef.current;
    const tilePositions = tilePositionsRef.current;
    const tileMap = tileMapRef.current;
    const mapGeometry = mapGeometryRef.current;
    const classifiedEdges = classifiedEdgesRef.current;
    const nationFillMap = nationFillMapRef.current;
    const seaZones = seaZonesRef.current;
    const mapDims = mapDimsRef.current;

    if (tiles.length === 0) return null;
    const PAD = HEX_SIZE * 1.5;
    const originX = staticBbox.minX - PAD;
    const originY = staticBbox.minY - PAD;
    const wWorld = (staticBbox.maxX - staticBbox.minX) + 2 * PAD;
    const hWorld = (staticBbox.maxY - staticBbox.minY) + 2 * PAD;
    const cw = Math.max(1, Math.ceil(wWorld * staticScaleBucket));
    const ch = Math.max(1, Math.ceil(hWorld * staticScaleBucket));
    // Hard cap on canvas size to avoid OOM at extreme zoom (browsers also
    // refuse to allocate canvases above ~16k on a side). At staticBucket=2
    // and a 200×200 map this would be ~14k×7k — comfortably under cap.
    const MAX_DIM = 12000;
    if (cw > MAX_DIM || ch > MAX_DIM) return null;
    const sc = document.createElement('canvas');
    sc.width = cw;
    sc.height = ch;
    const sctx = sc.getContext('2d');
    if (!sctx) return null;
    const end = perfMark(`staticLayer bake (bucket=${staticScaleBucket}, ${cw}×${ch})`);

    // World (originX, originY) → canvas (0, 0). Scale by bucket so 1 world
    // unit = bucket device px on the offscreen canvas.
    sctx.setTransform(staticScaleBucket, 0, 0, staticScaleBucket, -originX * staticScaleBucket, -originY * staticScaleBucket);

    // Use the bucketed scale for any pass logic that previously read the
    // live scale (line widths, label visibility gates). The composite blit
    // below stretches the result to live scale — small bucket-vs-live gaps
    // show as proportional resampling, not as gameplay changes.
    const bScale = staticScaleBucket;
    const zoomedInPastLabelsBucket = mapDims.mapPixelHeight > 0
      ? bScale > (canvasSize.h / mapDims.mapPixelHeight) * 1.5
      : false;

    // Helpers replicated from render() — same fill semantics.
    const pickPoliticalColor = (tile: TileData): string => {
      if (!tile.owner_color) return TERRAIN_COLORS[tile.terrain] || '#666';
      const nc = NATION_COLORS[tile.owner_color];
      if (!nc) return TERRAIN_COLORS[tile.terrain] || '#666';
      return tile.is_incorporated_minor ? incorporatedFill(nc) : politicalFill(nc);
    };
    const tileFillColor = (tile: TileData): string => {
      if (tile.terrain === 'Sea') return TERRAIN_COLORS.Sea;
      if (mapMode === 'terrain') {
        let color = TERRAIN_COLORS[tile.terrain] || '#666';
        if (tile.owner_color) {
          const nc = NATION_COLORS[tile.owner_color];
          if (nc) color = tintColor(color, nc, tile.is_incorporated_minor ? 0.10 : 0.15);
        }
        return color;
      }
      if (mapMode === 'diplomatic' || mapMode === 'relationship' ||
          mapMode === 'military' || mapMode === 'naval') {
        const overlayColor = tile.owner ? nationFillMap.get(tile.owner) : null;
        return overlayColor ?? pickPoliticalColor(tile);
      }
      return pickPoliticalColor(tile);
    };

    // Pass 1: Land fills (organic clipped or non-organic per-hex).
    if (mapGeometry) {
      for (let i = 0; i < mapGeometry.componentClips.length; i++) {
        const comp = mapGeometry.componentClips[i];
        const compKeys = comp.memberKeys;
        const first = compKeys.length > 0 ? tileMap.get(compKeys[0]) : undefined;
        if (first) {
          sctx.fillStyle = tileFillColor(first);
          sctx.fill(comp.path, 'evenodd');
        }
        sctx.save();
        sctx.clip(comp.path, 'evenodd');
        for (const key of compKeys) {
          const tile = tileMap.get(key);
          if (!tile) continue;
          const [px, py] = hexToPixel(tile.q, tile.r);
          drawHexagon(sctx, px, py, HEX_SIZE);
          sctx.fillStyle = tileFillColor(tile);
          sctx.fill();
        }
        sctx.restore();
      }
    } else {
      for (let i = 0; i < tilePositions.length; i++) {
        const { tile, px, py } = tilePositions[i];
        drawHexagon(sctx, px, py, HEX_SIZE);
        sctx.fillStyle = tileFillColor(tile);
        sctx.fill();
      }
    }

    // Fog of war — drawn over land fills, under sea zones (preserves the
    // existing layering where fogged sea hexes still show zone shading).
    if (!disableFogOfWar) {
      sctx.fillStyle = 'rgba(128, 128, 128, 0.35)';
      for (let i = 0; i < tilePositions.length; i++) {
        const { tile, px, py } = tilePositions[i];
        if (tile.visible) continue;
        drawHexagon(sctx, px, py, HEX_SIZE);
        sctx.fill();
      }
    }

    // Pass 1.5: Sea zones (fill + borders + zoom-gated labels).
    if (seaZones.length > 0) {
      const hexZoneMap = new Map<string, number>();
      for (const zone of seaZones) {
        for (const hex of zone.hexes) hexZoneMap.set(`${hex.q},${hex.r}`, zone.id);
      }
      sctx.fillStyle = SEA_ZONE_FILL_COLOR;
      for (const zone of seaZones) {
        for (const hex of zone.hexes) {
          const [px, py] = hexToPixel(hex.q, hex.r);
          drawHexagon(sctx, px, py, HEX_SIZE);
          sctx.fill();
        }
      }
      const verts = hexVertices(HEX_SIZE);
      sctx.strokeStyle = SEA_ZONE_BORDER_COLOR;
      sctx.lineWidth = 1.5 / bScale;
      sctx.lineCap = 'round';
      sctx.beginPath();
      for (const zone of seaZones) {
        for (const hex of zone.hexes) {
          const [px, py] = hexToPixel(hex.q, hex.r);
          const neighbors = hexNeighbors(hex.q, hex.r);
          for (let d = 0; d < 6; d++) {
            const [nq, nr] = neighbors[d];
            const nzId = hexZoneMap.get(`${nq},${nr}`);
            if (nzId !== undefined && nzId !== zone.id) {
              sctx.moveTo(px + verts[d][0], py + verts[d][1]);
              sctx.lineTo(px + verts[(d + 1) % 6][0], py + verts[(d + 1) % 6][1]);
            }
          }
        }
      }
      sctx.stroke();

      if (bScale > 0.4) {
        sctx.textAlign = 'center';
        sctx.textBaseline = 'middle';
        const labelFontSize = Math.max(9, Math.min(16, HEX_SIZE * bScale * 0.9));
        sctx.font = `italic ${labelFontSize / bScale}px Georgia, serif`;
        sctx.globalAlpha = 0.6;
        for (const zone of seaZones) {
          if (zone.hexes.length === 0) continue;
          const [cx, cy] = hexToPixel(zone.center_q, zone.center_r);
          const label = zone.name.toUpperCase();
          sctx.strokeStyle = 'rgba(0,0,0,0.5)';
          sctx.lineWidth = 2 / bScale;
          sctx.strokeText(label, cx, cy);
          sctx.fillStyle = 'rgba(200,230,255,0.95)';
          sctx.fillText(label, cx, cy);
        }
        sctx.globalAlpha = 1.0;
      }
    }

    // Pass 2: Edge strokes (hex grid + borders).
    {
      const { normalEdges, provinceEdges, countryEdges } = classifiedEdges;
      if (!hideHexGrid && bScale > 0.4) {
        sctx.strokeStyle = 'rgba(0,0,0,0.08)';
        sctx.lineWidth = 0.5;
        sctx.lineCap = 'butt';
        sctx.beginPath();
        for (let i = 0; i < normalEdges.length; i += 4) {
          sctx.moveTo(normalEdges[i], normalEdges[i + 1]);
          sctx.lineTo(normalEdges[i + 2], normalEdges[i + 3]);
        }
        sctx.stroke();
      }

      if (mapGeometry) {
        const strokePolyline = (pts: Vec2[], closed: boolean) => {
          if (pts.length < 2) return;
          sctx.beginPath();
          sctx.moveTo(pts[0][0], pts[0][1]);
          for (let k = 1; k < pts.length; k++) sctx.lineTo(pts[k][0], pts[k][1]);
          if (closed) sctx.closePath();
          sctx.stroke();
        };
        sctx.lineJoin = 'round';
        sctx.lineCap = 'round';
        if (mapMode !== 'diplomatic' && zoomedInPastLabelsBucket) {
          sctx.strokeStyle = 'rgba(20,15,10,0.5)';
          sctx.lineWidth = 1.5;
          for (const loop of mapGeometry.provincePolylinesClosed) strokePolyline(loop, true);
          for (const line of mapGeometry.provincePolylinesOpen) strokePolyline(line, false);
        }
        sctx.strokeStyle = 'rgba(10,5,0,0.9)';
        sctx.lineWidth = 3.5;
        for (const loop of mapGeometry.countryPolylinesClosed) strokePolyline(loop, true);
        for (const line of mapGeometry.countryPolylinesOpen) strokePolyline(line, false);
        sctx.strokeStyle = 'rgba(10,5,0,0.85)';
        sctx.lineWidth = 2.5;
        for (const loop of mapGeometry.coastPolylinesClosed) strokePolyline(loop, true);
        for (const line of mapGeometry.coastPolylinesOpen) strokePolyline(line, false);
      } else {
        if (mapMode !== 'diplomatic' && zoomedInPastLabelsBucket) {
          sctx.strokeStyle = 'rgba(20,15,10,0.5)';
          sctx.lineWidth = 1.5;
          sctx.beginPath();
          for (let i = 0; i < provinceEdges.length; i += 4) {
            sctx.moveTo(provinceEdges[i], provinceEdges[i + 1]);
            sctx.lineTo(provinceEdges[i + 2], provinceEdges[i + 3]);
          }
          sctx.stroke();
        }
        sctx.strokeStyle = 'rgba(10,5,0,0.9)';
        sctx.lineWidth = 3.5;
        sctx.beginPath();
        for (let i = 0; i < countryEdges.length; i += 4) {
          sctx.moveTo(countryEdges[i], countryEdges[i + 1]);
          sctx.lineTo(countryEdges[i + 2], countryEdges[i + 3]);
        }
        sctx.stroke();
      }
    }

    end();
    return { canvas: sc, originX, originY, scaleBucket: staticScaleBucket };
    // Pinned to *content* signatures rather than memo refs. The wasm bridge
    // hands us a fresh `tiles` / `seaZones` / `diplomacyOverlay` etc. on
    // every poll even when content is unchanged; depending on those refs
    // would re-bake every poll (including during pan, when polls happen to
    // fire). The signatures below recompute O(n) each render but only flip
    // values when something *meaningful* has changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    borderSignature, fillSignature, fogSignature,
    seaZonesSignature, nationFillSignature,
    mapMode, hideHexGrid, organicBorders, disableFogOfWar,
    staticBbox, staticScaleBucket, canvasSize.h,
  ]);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const endRender = perfMark(`render (scale=${scaleRef.current.toFixed(2)})`);

    // Canvas size is assigned by the ResizeObserver (not every frame).
    // Fill the whole canvas with sea color so any region outside the map's
    // own seaBox (north / south vertical bars when the map is shorter than
    // the canvas, plus the usual seam zones at wrap boundaries) inherits the
    // same backdrop as the map's oceans — the map reads as a continental
    // region floating on an endless sea, not a sprite on the app background.
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = TERRAIN_COLORS.Sea;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Read gesture-authoritative transform from refs so zoom/pan during
    // a gesture can update refs directly and schedule a frame without a
    // React state commit.
    const scale = scaleRef.current;
    const offset = offsetRef.current;

    // Horizontal wrap: draw the entire per-frame content once per wrap copy
    // whose screen rect overlaps the viewport. The transform shifts world x
    // by k*mapPixelWidth per copy; everything drawn in world space (tiles,
    // borders, labels, units, overlays) automatically duplicates. Overlays
    // drawn in screen space are outside this loop.
    //
    // A single copy's tiles span world x wider than one wrap period because
    // pointy-top rows shift by SQRT3/2*HEX_SIZE per r. At r=mapHeight-1 the
    // row is offset right by (mapHeight-1)*SQRT3/2*HEX_SIZE, leaving the
    // viewport's bottom-left corner uncovered unless we also draw copy k-ext
    // (whose own high-r tiles reach back into that column). The overflow
    // fraction (rowOffset / mapPixelWidth) is ≈0.3 for typical aspect
    // ratios; we extend kMin by ceil(overflow) so the full viewport is
    // covered at every row.
    const { mapPixelWidth, mapHeight } = mapDims;
    const periodScreen = mapPixelWidth * scale;
    let kMin = 0, kMax = 0;
    if (periodScreen > 0 && canvas.width > 0) {
      const rowOffsetSpan = Math.max(0, mapHeight - 1) * (SQRT3 / 2) * HEX_SIZE;
      const kExtraLeft = mapPixelWidth > 0
        ? Math.ceil(rowOffsetSpan / mapPixelWidth)
        : 0;
      kMin = Math.floor(-offset.x / periodScreen) - kExtraLeft;
      kMax = Math.ceil((canvas.width - offset.x) / periodScreen);
      // Safety cap: if the clamp effect has't run yet (transient state during
      // tile-load), the computed range could be dozens of copies wide and
      // each copy re-walks every tile. Clamp to 8 copies centered on the
      // viewport so a malformed state only drops extra copies, not frames.
      const MAX_COPIES = 8;
      if (kMax - kMin + 1 > MAX_COPIES) {
        const viewportCenterK = Math.round(((canvas.width / 2) - offset.x) / periodScreen);
        kMin = viewportCenterK - Math.floor(MAX_COPIES / 2);
        kMax = kMin + MAX_COPIES - 1;
      }
    }

    // Helper: pick the right political fill based on incorporated status
    const pickPoliticalColor = (tile: TileData): string => {
      if (!tile.owner_color) return TERRAIN_COLORS[tile.terrain] || '#666';
      const nc = NATION_COLORS[tile.owner_color];
      if (!nc) return TERRAIN_COLORS[tile.terrain] || '#666';
      return tile.is_incorporated_minor ? incorporatedFill(nc) : politicalFill(nc);
    };

    // Resolve the fill color for a single tile in the current map mode.
    const tileFillColor = (tile: TileData): string => {
      if (tile.terrain === 'Sea') return TERRAIN_COLORS.Sea;
      if (mapMode === 'terrain') {
        let color = TERRAIN_COLORS[tile.terrain] || '#666';
        if (tile.owner_color) {
          const nc = NATION_COLORS[tile.owner_color];
          if (nc) color = tintColor(color, nc, tile.is_incorporated_minor ? 0.10 : 0.15);
        }
        return color;
      }
      if (mapMode === 'diplomatic' || mapMode === 'relationship' ||
          mapMode === 'military' || mapMode === 'naval') {
        const overlayColor = tile.owner ? nationFillMap.get(tile.owner) : null;
        return overlayColor ?? pickPoliticalColor(tile);
      }
      return pickPoliticalColor(tile);
    };

    const fitScaleForLabels = mapDims.mapPixelHeight > 0 && canvas.height > 0
      ? canvas.height / mapDims.mapPixelHeight
      : 0;
    const zoomedInPastLabels = fitScaleForLabels > 0 && scale > fitScaleForLabels * 1.5;

    // Frustum-culling pad: a hex's bounding box extends HEX_SIZE*SQRT3 in x
    // (across the wide flat-side dimension of a pointy-top hex) and HEX_SIZE*2
    // in y. Add a small margin so anti-aliased edges aren't clipped.
    const cullPadX = HEX_SIZE * SQRT3 + 2;
    const cullPadY = HEX_SIZE * 2 + 2;

    for (let k = kMin; k <= kMax; k++) {
      ctx.setTransform(scale, 0, 0, scale, offset.x + k * periodScreen, offset.y);

    // Visible world rect for this wrap copy. (px, py) → screen is
    // (scale*px + offset.x + k*periodScreen, scale*py + offset.y); invert to
    // get the world-x range that maps onto [0, canvas.width] and likewise y.
    const worldXMin = (-offset.x - k * periodScreen) / scale - cullPadX;
    const worldXMax = (canvas.width - offset.x - k * periodScreen) / scale + cullPadX;
    const worldYMin = (-offset.y) / scale - cullPadY;
    const worldYMax = (canvas.height - offset.y) / scale + cullPadY;

    // visibleTiles: only the hex centers inside the viewport rect for this
    // wrap copy. Used by every per-tile pass below to avoid touching the
    // ~thousands of off-screen hexes when the user is zoomed in.
    const visibleTiles: TileData[] = [];
    for (let i = 0; i < tilePositions.length; i++) {
      const tp = tilePositions[i];
      if (tp.px < worldXMin || tp.px > worldXMax) continue;
      if (tp.py < worldYMin || tp.py > worldYMax) continue;
      visibleTiles.push(tp.tile);
    }
    // If this wrap copy contributes no visible tiles, skip the rest of the
    // per-copy work (sea zones, borders, label passes also gain no value when
    // there's nothing on-screen for this copy).
    if (visibleTiles.length === 0) continue;

    // Static blit: terrain fills, fog, sea zones, hex grid, and borders all
    // come from the offscreen cache. Single drawImage per wrap copy replaces
    // the four heaviest per-frame passes. Stretch by (scale / scaleBucket)
    // when the live scale differs from the bake bucket; the resampling is
    // imperceptible within the 25% bucket window.
    if (staticLayer) {
      const ratio = scale / staticLayer.scaleBucket;
      const dstW = staticLayer.canvas.width * ratio;
      const dstH = staticLayer.canvas.height * ratio;
      // Reset to identity so drawImage args are in screen px (drawImage
      // honors the active transform; using identity makes the math obvious).
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      const dstX = staticLayer.originX * scale + offset.x + k * periodScreen;
      const dstY = staticLayer.originY * scale + offset.y;
      ctx.drawImage(staticLayer.canvas, dstX, dstY, dstW, dstH);
      // Restore the per-copy world transform for the dynamic passes below.
      ctx.setTransform(scale, 0, 0, scale, offset.x + k * periodScreen, offset.y);
    } else if (mapGeometry) {
      // Static cache unavailable (e.g. canvas allocation failed) — fall back
      // to inline organic-mode pass 1.
      for (let i = 0; i < mapGeometry.componentClips.length; i++) {
        const comp = mapGeometry.componentClips[i];
        const compKeys = comp.memberKeys;
        // Fatten pass: fill the full component polygon with a representative
        // color so the boundary anti-aliasing zone ends up land-tinted
        // instead of sea-tinted. Without this, the AA-blended pixels at the
        // smoothed edge let the sea background bleed through as a blue
        // sliver below the stroke.
        const first = compKeys.length > 0 ? tileMap.get(compKeys[0]) : undefined;
        if (first) {
          ctx.fillStyle = tileFillColor(first);
          ctx.fill(comp.path, 'evenodd');
        }
        ctx.save();
        ctx.clip(comp.path, 'evenodd');
        for (const key of compKeys) {
          const tile = tileMap.get(key);
          if (!tile) continue;
          const [px, py] = hexToPixel(tile.q, tile.r);
          drawHexagon(ctx, px, py, HEX_SIZE);
          ctx.fillStyle = tileFillColor(tile);
          ctx.fill();
        }
        ctx.restore();
      }
    } else {
      // ── Original non-organic rendering: per-hex fills for every tile ──
      for (const tile of visibleTiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.fillStyle = tileFillColor(tile);
        ctx.fill();
      }
    }

    // Fallback path: when staticLayer is unavailable, fog + sea zones +
    // borders are drawn inline. The static cache normally renders these
    // ahead of time so they're already in the blit above.
    if (!staticLayer) {
    // Fog of war — applied per-hex (land and sea) in both modes.
    if (!disableFogOfWar) {
      ctx.fillStyle = 'rgba(128, 128, 128, 0.35)';
      for (const tile of visibleTiles) {
        if (tile.visible) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.fill();
      }
    }

    // ── Pass 1.5: Sea zone shading ────────────────────────────────
    if (seaZones.length > 0) {
      // Build hex-key → zone id lookup for border detection.
      const hexZoneMap = new Map<string, number>();
      for (const zone of seaZones) {
        for (const hex of zone.hexes) {
          hexZoneMap.set(`${hex.q},${hex.r}`, zone.id);
        }
      }

      // Uniform fill for all zone hexes.
      ctx.fillStyle = SEA_ZONE_FILL_COLOR;
      for (const zone of seaZones) {
        for (const hex of zone.hexes) {
          const [px, py] = hexToPixel(hex.q, hex.r);
          drawHexagon(ctx, px, py, HEX_SIZE);
          ctx.fill();
        }
      }

      // Darker border on every edge where two different zones meet.
      const verts = hexVertices(HEX_SIZE);
      ctx.strokeStyle = SEA_ZONE_BORDER_COLOR;
      ctx.lineWidth = 1.5 / scale;
      ctx.lineCap = 'round';
      ctx.beginPath();
      for (const zone of seaZones) {
        for (const hex of zone.hexes) {
          const [px, py] = hexToPixel(hex.q, hex.r);
          const neighbors = hexNeighbors(hex.q, hex.r);
          for (let d = 0; d < 6; d++) {
            const [nq, nr] = neighbors[d];
            const nzId = hexZoneMap.get(`${nq},${nr}`);
            if (nzId !== undefined && nzId !== zone.id) {
              ctx.moveTo(px + verts[d][0], py + verts[d][1]);
              ctx.lineTo(px + verts[(d + 1) % 6][0], py + verts[(d + 1) % 6][1]);
            }
          }
        }
      }
      ctx.stroke();

      // Zone name labels (only when zoomed in enough).
      if (scale > 0.4) {
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        const labelFontSize = Math.max(9, Math.min(16, HEX_SIZE * scale * 0.9));
        ctx.font = `italic ${labelFontSize / scale}px Georgia, serif`;
        ctx.globalAlpha = 0.6;
        for (const zone of seaZones) {
          if (zone.hexes.length === 0) continue;
          const [cx, cy] = hexToPixel(zone.center_q, zone.center_r);
          const label = zone.name.toUpperCase();
          ctx.strokeStyle = 'rgba(0,0,0,0.5)';
          ctx.lineWidth = 2 / scale;
          ctx.strokeText(label, cx, cy);
          ctx.fillStyle = 'rgba(200,230,255,0.95)';
          ctx.fillText(label, cx, cy);
        }
        ctx.globalAlpha = 1.0;
      }
    }

    // ── Pass 2: Edge strokes ──
    // When organicBorders is on, interior same-province edges are drawn as a
    // subtle hex grid (gameplay aid), and the three political/coast classes
    // are drawn as smoothed polylines. When off, all four classes are drawn
    // as straight hex-edge segments in their original styles.
    {
      const { normalEdges, provinceEdges, countryEdges } = classifiedEdges;

      // Thin subtle intra-province hex grid — drawn straight in both modes
      // (hex edges that aren't at a border stay hexagonal by design). Can be
      // hidden entirely via the Hide Hex Grid toggle.
      // LOD: 0.5px-wide strokes at scale < 0.4 fall below ~0.2 screen px and
      // are imperceptible against the fog/border layers — skip the path build
      // entirely instead of paying the per-edge cost for invisible output.
      if (!hideHexGrid && scale > 0.4) {
        ctx.strokeStyle = 'rgba(0,0,0,0.08)';
        ctx.lineWidth = 0.5;
        ctx.lineCap = 'butt';
        ctx.beginPath();
        for (let i = 0; i < normalEdges.length; i += 4) {
          ctx.moveTo(normalEdges[i], normalEdges[i + 1]);
          ctx.lineTo(normalEdges[i + 2], normalEdges[i + 3]);
        }
        ctx.stroke();
      }

      if (mapGeometry) {
        const strokePolyline = (pts: Vec2[], closed: boolean) => {
          if (pts.length < 2) return;
          ctx.beginPath();
          ctx.moveTo(pts[0][0], pts[0][1]);
          for (let k = 1; k < pts.length; k++) ctx.lineTo(pts[k][0], pts[k][1]);
          if (closed) ctx.closePath();
          ctx.stroke();
        };
        ctx.lineJoin = 'round';
        ctx.lineCap = 'round';

        if (mapMode !== 'diplomatic' && zoomedInPastLabels) {
          ctx.strokeStyle = 'rgba(20,15,10,0.5)';
          ctx.lineWidth = 1.5;
          for (const loop of mapGeometry.provincePolylinesClosed) strokePolyline(loop, true);
          for (const line of mapGeometry.provincePolylinesOpen) strokePolyline(line, false);
        }

        ctx.strokeStyle = 'rgba(10,5,0,0.9)';
        ctx.lineWidth = 3.5;
        for (const loop of mapGeometry.countryPolylinesClosed) strokePolyline(loop, true);
        for (const line of mapGeometry.countryPolylinesOpen) strokePolyline(line, false);

        ctx.strokeStyle = 'rgba(10,5,0,0.85)';
        ctx.lineWidth = 2.5;
        for (const loop of mapGeometry.coastPolylinesClosed) strokePolyline(loop, true);
        for (const line of mapGeometry.coastPolylinesOpen) strokePolyline(line, false);
      } else {
        // Straight hex-edge strokes — original look.
        if (mapMode !== 'diplomatic' && zoomedInPastLabels) {
          ctx.strokeStyle = 'rgba(20,15,10,0.5)';
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          for (let i = 0; i < provinceEdges.length; i += 4) {
            ctx.moveTo(provinceEdges[i], provinceEdges[i + 1]);
            ctx.lineTo(provinceEdges[i + 2], provinceEdges[i + 3]);
          }
          ctx.stroke();
        }

        ctx.strokeStyle = 'rgba(10,5,0,0.9)';
        ctx.lineWidth = 3.5;
        ctx.beginPath();
        for (let i = 0; i < countryEdges.length; i += 4) {
          ctx.moveTo(countryEdges[i], countryEdges[i + 1]);
          ctx.lineTo(countryEdges[i + 2], countryEdges[i + 3]);
        }
        ctx.stroke();
      }
    }
    } // end !staticLayer fallback for fog+seaZones+borders

    // ── Pass 2.5: Highlight selected nation (setup preview) ──
    // In organic mode, stroke the smoothed outline of each of the nation's
    // components as a single red contour. In non-organic mode, fall back to
    // per-hex outlines (skipping sea tiles — that was the bug where picking
    // a nation whose id collided with sea-tile data lit up the whole ocean).
    if (highlightedNationId != null) {
      if (mapGeometry) {
        ctx.strokeStyle = PREVIEW_HIGHLIGHT_COLOR;
        ctx.lineWidth = PREVIEW_HIGHLIGHT_WIDTH;
        ctx.lineJoin = 'round';
        ctx.lineCap = 'round';
        for (let i = 0; i < mapGeometry.componentClips.length; i++) {
          const comp = mapGeometry.componentClips[i];
          const compKeys = comp.memberKeys;
          // Resolve to live tiles via tileMap — nation_id is not in
          // borderSignature so the cached memberKeys may outlive a nation_id
          // change on those tiles.
          let matches = false;
          for (const key of compKeys) {
            const t = tileMap.get(key);
            if (t && t.nation_id === highlightedNationId) { matches = true; break; }
          }
          if (!matches) continue;
          ctx.stroke(comp.path);
        }
      } else {
        ctx.strokeStyle = PREVIEW_HIGHLIGHT_COLOR;
        ctx.lineWidth = PREVIEW_HIGHLIGHT_WIDTH;
        ctx.lineCap = 'butt';
        ctx.lineJoin = 'miter';
        for (const tile of visibleTiles) {
          if (tile.terrain === 'Sea') continue;
          if (tile.nation_id !== highlightedNationId) continue;
          const [px, py] = hexToPixel(tile.q, tile.r);
          drawHexagon(ctx, px, py, HEX_SIZE * 0.95);
          ctx.stroke();
        }
      }
    }

    // ── Pass 3: Capitals ──
    for (const tile of visibleTiles) {
      if (!tile.is_capital || tile.terrain === 'Sea') continue;
      const [px, py] = hexToPixel(tile.q, tile.r);

      if (tile.is_country_capital) {
        const sz = Math.max(15, HEX_SIZE * 0.9);
        ctx.font = `bold ${sz}px serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.lineWidth = 2.5;
        ctx.strokeStyle = 'rgba(0,0,0,0.8)';
        ctx.strokeText('\u2605', px, py);
        ctx.fillStyle = '#ffd700';
        ctx.fillText('\u2605', px, py);
      } else {
        ctx.beginPath();
        ctx.arc(px, py, 2.5, 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(255,255,255,0.7)';
        ctx.fill();
        ctx.strokeStyle = 'rgba(0,0,0,0.4)';
        ctx.lineWidth = 0.8;
        ctx.stroke();
      }
    }

    // ── Pass 4: Resource icons on producing tiles (terrain view only) ──
    if (showResources && scale > 0.6 && mapMode === 'terrain') {
      const rSize = Math.max(10, HEX_SIZE * 0.7);
      const badgeFont = Math.max(7, HEX_SIZE * 0.32);
      const resourceFontStr = `${rSize}px sans-serif`;
      const badgeFontStr = `bold ${badgeFont}px sans-serif`;
      // Text alignment is uniform across the pass — set once. Font alternates
      // between resource and badge inside the loop, but only when a badge
      // actually needs drawing (skips the extra set on plain icons).
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.font = resourceFontStr;
      for (const tile of visibleTiles) {
        if (tile.terrain === 'Sea' || !tile.owner) continue;
        if (tile.is_capital || tile.is_country_capital) continue;
        // Skip hidden resources unless debug toggle is on
        if (tile.resource_hidden && !showHiddenResources) continue;
        const icon = getResourceIcon(tile);
        if (!icon) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);

        ctx.globalAlpha = tile.resource_hidden ? (showHiddenResources ? 0.85 : 0.4) : 0.75;
        ctx.fillText(icon, px, py);
        ctx.globalAlpha = 1.0;

        // Improvement-level badge (e.g. "2/3"), gold when fully improved
        if (tile.improvement_level > 0 && tile.max_improvement_level > 0) {
          ctx.font = badgeFontStr;
          const fully = tile.improvement_level >= tile.max_improvement_level;
          const text = `${tile.improvement_level}/${tile.max_improvement_level}`;
          const bx = px + HEX_SIZE * 0.5;
          const by = py + HEX_SIZE * 0.55;
          ctx.lineWidth = 3;
          ctx.strokeStyle = 'rgba(0,0,0,0.85)';
          ctx.strokeText(text, bx, by);
          ctx.fillStyle = fully ? '#ffd700' : '#fff';
          ctx.fillText(text, bx, by);
          ctx.font = resourceFontStr;
        }
      }
      ctx.globalAlpha = 1.0;
    }

    // ── Pass 5: Infrastructure icons ──
    if (scale > 0.8) {
      const iconSize = Math.max(8, HEX_SIZE * 0.5);
      const iconFontStr = `${iconSize}px sans-serif`;
      // Font + alignment are uniform across the pass except for forts, which
      // use a size that depends on fort_level and re-sets font inside the
      // loop. Set defaults once here.
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.font = iconFontStr;
      for (const tile of visibleTiles) {
        if (tile.terrain === 'Sea') continue;
        if (!tile.has_railroad && !tile.has_depot && !tile.has_port && !tile.has_fort) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);

        if (showTransportNetwork && tile.has_railroad && mapMode === 'terrain') {
          // Draw small railroad tracks (two parallel lines)
          const rw = HEX_SIZE * 0.35;
          ctx.strokeStyle = 'rgba(100,60,20,0.8)';
          ctx.lineWidth = 1.2;
          ctx.beginPath();
          ctx.moveTo(px - rw, py - 2); ctx.lineTo(px + rw, py - 2);
          ctx.moveTo(px - rw, py + 2); ctx.lineTo(px + rw, py + 2);
          // Cross-ties
          for (let t = -rw + 2; t <= rw - 2; t += 5) {
            ctx.moveTo(px + t, py - 3); ctx.lineTo(px + t, py + 3);
          }
          ctx.stroke();
        }
        if (showTransportNetwork && tile.has_depot && mapMode === 'terrain') {
          ctx.fillStyle = 'rgba(139,90,43,0.9)';
          const ds = HEX_SIZE * 0.2;
          ctx.fillRect(px - ds + HEX_SIZE * 0.3, py - ds, ds * 2, ds * 2);
          ctx.strokeStyle = 'rgba(0,0,0,0.6)';
          ctx.lineWidth = 0.8;
          ctx.strokeRect(px - ds + HEX_SIZE * 0.3, py - ds, ds * 2, ds * 2);
        }
        if (showTransportNetwork && tile.has_port) {
          const ax = px - HEX_SIZE * 0.3;
          ctx.lineWidth = 1;
          ctx.strokeStyle = 'rgba(0,0,0,0.6)';
          ctx.strokeText('\u2693', ax, py);
          // Red anchor when blockaded (card #408), blue otherwise.
          ctx.fillStyle = tile.port_blockaded ? 'rgba(200,40,40,0.95)' : 'rgba(70,130,200,0.9)';
          ctx.fillText('\u2693', ax, py);
        }
        if (tile.has_fort) {
          const fl = tile.fort_level || 1;
          const fs = iconSize * (0.8 + fl * 0.2);
          ctx.font = `${fs}px sans-serif`;
          ctx.lineWidth = 1;
          ctx.strokeStyle = 'rgba(0,0,0,0.7)';
          ctx.strokeText('\u26ED', px, py - HEX_SIZE * 0.3);
          ctx.fillStyle = `rgba(120,120,130,0.9)`;
          ctx.fillText('\u26ED', px, py - HEX_SIZE * 0.3);
          // Reset to default icon font so the next tile's icons don't
          // inherit this fort's fort_level-dependent size.
          ctx.font = iconFontStr;
        }
      }
    }

    // ── Pass 5b: Diplomatic presence icons (consulate/embassy) ──
    // Anchored below the nation label centroid (not the capital tile) so the
    // emoji appears directly under the country name as the card requires.
    if (showDiplomacyMarkers && diplomacyOverlay) {
      const diploByNation = new Map<string, typeof diplomacyOverlay.relations[0]>();
      for (const rel of diplomacyOverlay.relations) {
        diploByNation.set(rel.nation_name, rel);
      }

      // Use a large emoji size so the icon is clearly prominent on the map.
      const emojiSize = Math.max(18, HEX_SIZE * 1.2);
      ctx.font = `${emojiSize}px sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';

      for (const label of nationLabels) {
        const rel = diploByNation.get(label.name);
        if (!rel) continue;
        if (!rel.has_consulate && !rel.has_embassy) continue;

        // Place emoji just below the nation name label centroid.
        const fontSize = Math.max(12, Math.min(28, Math.sqrt(label.size) * 3));
        const iy = label.cy + fontSize * 0.6;

        const emoji = rel.has_embassy ? '\u{1F3DB}️' : '\u{1F4DC}'; // 🏛️ embassy, 📜 consulate
        ctx.fillText(emoji, label.cx, iy);
      }
      ctx.textBaseline = 'middle';
    }

    // ── Pass 6: Nation name labels (all non-terrain modes, hidden when zoomed in) ──
    if (showPoliticalColors && !zoomedInPastLabels) {
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      for (const label of nationLabels) {
        const fontSize = Math.max(12, Math.min(28, Math.sqrt(label.size) * 3));
        ctx.font = `bold ${fontSize}px Georgia, serif`;
        ctx.lineWidth = 3;
        if (label.is_anarchic) {
          ctx.strokeStyle = 'rgba(255,255,255,0.55)';
          ctx.strokeText(label.name.toUpperCase(), label.cx, label.cy);
          ctx.fillStyle = 'rgba(0,0,0,0.95)';
        } else {
          ctx.strokeStyle = 'rgba(0,0,0,0.5)';
          ctx.strokeText(label.name.toUpperCase(), label.cx, label.cy);
          ctx.fillStyle = 'rgba(255,255,255,0.9)';
        }
        ctx.fillText(label.name.toUpperCase(), label.cx, label.cy);
      }
    }

    // ── Pass 6b: Province names (only when zoomed in past country-name threshold) ──
    if (showPoliticalColors && zoomedInPastLabels) {
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      for (const label of provinceLabels) {
        const fontSize = Math.max(7, Math.min(14, Math.sqrt(label.size) * 2.5));
        ctx.font = `${fontSize}px Georgia, serif`;
        ctx.lineWidth = 2;
        ctx.strokeStyle = 'rgba(0,0,0,0.55)';
        ctx.strokeText(label.name, label.cx, label.cy);
        ctx.fillStyle = 'rgba(230,220,190,0.9)';
        ctx.fillText(label.name, label.cx, label.cy);
      }
    }

    // ── Pass 7: Troop emoji indicators at capitals ──────────────
    // Single ⚔️ emoji for all nation types; font size scales with unit count.
    // Selected tile's indicator blinks.
    if (showArmies && scale > 0.6 && mapMode !== 'diplomatic') {
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      for (const tile of visibleTiles) {
        if (tile.terrain === 'Sea') continue;
        if (!tile.is_capital) continue;
        if (tile.army_unit_count === 0) continue;

        const tKey = `${tile.q},${tile.r}`;
        const isSelected = tKey === selectedTileKey;

        // Skip on odd blink frames for selected tile
        if (isSelected && !blinkOn) continue;

        const n = tile.army_unit_count;
        // Scale emoji size with unit count: 1 unit → 55%, 7+ units → 110% of HEX_SIZE
        const sizeScale = Math.min(1.1, 0.55 + n * 0.08);
        const emojiSize = Math.max(7, HEX_SIZE * sizeScale);

        const [px, py] = hexToPixel(tile.q, tile.r);
        // Position to the upper-right of the capital icon
        const ex = px + HEX_SIZE * 0.6;
        const ey = py - HEX_SIZE * 0.55;

        if (isSelected) {
          ctx.beginPath();
          ctx.arc(ex, ey, emojiSize * 0.65, 0, Math.PI * 2);
          ctx.fillStyle = 'rgba(255,220,0,0.35)';
          ctx.fill();
        }

        ctx.globalAlpha = isSelected ? 1.0 : 0.85;

        // Draw sword emoji
        ctx.font = `${emojiSize}px sans-serif`;
        ctx.fillText('⚔️', ex, ey);

        // Draw unit count below emoji
        const countSize = Math.max(6, emojiSize * 0.65);
        ctx.font = `bold ${countSize}px sans-serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillStyle = '#fff';
        ctx.lineWidth = 2;
        ctx.strokeStyle = 'rgba(0,0,0,0.8)';
        ctx.strokeText(String(n), ex, ey + emojiSize * 0.4);
        ctx.fillText(String(n), ex, ey + emojiSize * 0.4);
        ctx.textBaseline = 'middle';
        ctx.lineWidth = 1;

        ctx.globalAlpha = 1.0;
      }
      ctx.globalAlpha = 1.0;
    }

    // ── Pass 8: Civilian emoji icons on hex tiles ──────────────
    if (scale > 0.7) {
      const civFontSize = Math.max(6, HEX_SIZE * 0.55);
      ctx.font = `${civFontSize}px sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      for (const tile of visibleTiles) {
        if (!tile.civilian_on_tile) continue;
        // Skip AI civilians unless toggle is on
        if (!tile.civilian_on_tile.is_human && !showAiCivilians) continue;

        const [px, py] = hexToPixel(tile.q, tile.r);
        const emoji = CIVILIAN_EMOJI[tile.civilian_on_tile.type] || '\u{1F464}';

        // Position in lower-left of hex to avoid resource icons (center)
        const cx = px - HEX_SIZE * 0.3;
        const cy = py + HEX_SIZE * 0.35;

        // Draw nation-colored circle behind civilian emoji
        const civColor = NATION_COLORS[tile.civilian_on_tile.owner_color];
        if (civColor) {
          ctx.beginPath();
          ctx.arc(cx, cy, civFontSize * 0.45, 0, Math.PI * 2);
          ctx.fillStyle = civColor;
          ctx.globalAlpha = 0.5;
          ctx.fill();
          ctx.globalAlpha = 1.0;
        }

        ctx.fillText(emoji, cx, cy);

        // Turns remaining badge + construction-in-progress emoji
        if (tile.civilian_on_tile.working && tile.civilian_on_tile.turns_remaining > 0) {
          // In-progress emoji (left of the civilian icon)
          const task = tile.civilian_on_tile.build_task;
          const civType = tile.civilian_on_tile.type;
          const progressEmoji = (task && IN_PROGRESS_EMOJI_BY_TASK[task])
            || IN_PROGRESS_EMOJI_BY_CIV[civType]
            || IN_PROGRESS_FALLBACK;
          const progX = cx - civFontSize * 0.7;
          const progY = cy;
          ctx.font = `${civFontSize * 0.85}px sans-serif`;
          ctx.fillText(progressEmoji, progX, progY);
          ctx.font = `${civFontSize}px sans-serif`;

          // Turns-remaining badge (upper-right of civilian icon)
          const badgeX = cx + civFontSize * 0.5;
          const badgeY = cy - civFontSize * 0.4;
          ctx.fillStyle = 'rgba(0,0,0,0.7)';
          ctx.beginPath();
          ctx.arc(badgeX, badgeY, 4, 0, Math.PI * 2);
          ctx.fill();
          ctx.font = '5px sans-serif';
          ctx.fillStyle = '#fff';
          ctx.fillText(`${tile.civilian_on_tile.turns_remaining}`, badgeX, badgeY);
          ctx.font = `${civFontSize}px sans-serif`;
        }
      }
    }

    // ── Pass 8b: Navy markers (one per owner/fleet + per beachhead) ──
    if (navyMarkers.length > 0) {
      for (const m of navyMarkers) {
        const [basePx, basePy] = hexToPixel(m.q, m.r);
        const [dxo, dyo] = navyMarkerOffset(markerAnchorIndex.get(navyMarkerKey(m)) ?? 0);
        const px = basePx + dxo;
        const py = basePy + dyo;
        const isSelected = selectedNavyKey === navyMarkerKey(m);

        // Beachhead markers get a thin line toward the coast tile.
        if (m.kind === 'beachhead' && m.target_hex) {
          const [tx, ty] = hexToPixel(m.target_hex.q, m.target_hex.r);
          ctx.strokeStyle = 'rgba(230, 38, 38, 0.85)';
          ctx.lineWidth = 1.4;
          ctx.setLineDash([2, 2]);
          ctx.beginPath();
          ctx.moveTo(px, py);
          ctx.lineTo(tx, ty);
          ctx.stroke();
          ctx.setLineDash([]);
        }

        // Filled circle colored by owner.
        ctx.beginPath();
        ctx.arc(px, py, NAVY_MARKER_RADIUS, 0, Math.PI * 2);
        const fill = NATION_COLORS[m.owner_color] || '#888';
        ctx.fillStyle = fill;
        ctx.fill();

        // Border: red for beachhead, yellow for selected, white otherwise.
        ctx.lineWidth = isSelected ? 2.5 : 1.5;
        ctx.strokeStyle = m.kind === 'beachhead'
          ? '#e62626'
          : (isSelected ? '#ffd900' : 'rgba(255,255,255,0.9)');
        ctx.stroke();

        // Anchor glyph.
        ctx.font = '12px system-ui';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = '#111';
        ctx.fillText('\u2693', px, py + 0.5);

        // Ship count badge top-right.
        const badge = String(m.ship_count);
        ctx.font = 'bold 10px system-ui';
        const bx = px + NAVY_MARKER_RADIUS - 2;
        const by = py - NAVY_MARKER_RADIUS + 2;
        ctx.fillStyle = '#111';
        ctx.beginPath();
        ctx.arc(bx, by, 7, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = '#ffd900';
        ctx.fillText(badge, bx, by + 0.5);
      }
    }

    // ── Pass 9: Movement range highlighting ───────────────────
    if (isMovementMode && validMoveTargets) {
      const provinceIdSet = new Map<number, 'friendly' | 'hostile'>();
      for (const t of validMoveTargets.friendly) provinceIdSet.set(t.province_id, 'friendly');
      for (const t of validMoveTargets.hostile) provinceIdSet.set(t.province_id, 'hostile');

      for (const tile of visibleTiles) {
        if (tile.province_id == null) continue;
        const kind = provinceIdSet.get(tile.province_id);
        if (!kind) continue;

        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE - 0.5);
        ctx.fillStyle = kind === 'friendly'
          ? 'rgba(46, 204, 64, 0.25)'
          : 'rgba(255, 65, 54, 0.25)';
        ctx.fill();
      }
    }

    // ── Pass 9b: Deploy mode tile highlighting ────────────────
    if (isDeployMode && deployableTiles && deployableTiles.size > 0) {
      for (const tile of visibleTiles) {
        const key = `${tile.q},${tile.r}`;
        if (!deployableTiles.has(key)) continue;

        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE - 0.5);
        ctx.fillStyle = 'rgba(46, 204, 64, 0.3)';
        ctx.fill();
      }
    }

    // ── Pass 10: Movement arrows for pending moves ────────────
    if (pendingMoves.length > 0) {
      // Build province_id → capital tile pixel position lookup
      const capitalPositions = new Map<number, [number, number]>();
      for (const tile of tiles) {
        if (tile.is_capital && tile.province_id != null) {
          capitalPositions.set(tile.province_id, hexToPixel(tile.q, tile.r));
        }
      }

      ctx.setLineDash([]);
      ctx.lineWidth = 5;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      for (const move of pendingMoves) {
        const from = capitalPositions.get(move.source_province_id);
        const to = capitalPositions.get(move.dest_province_id);
        if (!from || !to) continue;

        const angle = Math.atan2(to[1] - from[1], to[0] - from[0]);
        const arrowLen = 18;
        const arrowAngle = 0.45;

        // Pull the shaft endpoint back so the line meets the arrowhead cleanly
        const shaftEndX = to[0] - Math.cos(angle) * arrowLen * 0.55;
        const shaftEndY = to[1] - Math.sin(angle) * arrowLen * 0.55;

        // Dark outline for contrast
        ctx.strokeStyle = 'rgba(0, 40, 0, 0.9)';
        ctx.lineWidth = 8;
        ctx.beginPath();
        ctx.moveTo(from[0], from[1]);
        ctx.lineTo(shaftEndX, shaftEndY);
        ctx.stroke();

        // Bright green shaft
        ctx.strokeStyle = 'rgba(72, 220, 90, 0.95)';
        ctx.lineWidth = 5;
        ctx.beginPath();
        ctx.moveTo(from[0], from[1]);
        ctx.lineTo(shaftEndX, shaftEndY);
        ctx.stroke();

        // Filled arrowhead
        const tipX = to[0];
        const tipY = to[1];
        const leftX = tipX - arrowLen * Math.cos(angle - arrowAngle);
        const leftY = tipY - arrowLen * Math.sin(angle - arrowAngle);
        const rightX = tipX - arrowLen * Math.cos(angle + arrowAngle);
        const rightY = tipY - arrowLen * Math.sin(angle + arrowAngle);
        ctx.beginPath();
        ctx.moveTo(tipX, tipY);
        ctx.lineTo(leftX, leftY);
        ctx.lineTo(rightX, rightY);
        ctx.closePath();
        ctx.fillStyle = 'rgba(72, 220, 90, 0.95)';
        ctx.strokeStyle = 'rgba(0, 40, 0, 0.9)';
        ctx.lineWidth = 1.5;
        ctx.fill();
        ctx.stroke();
      }
      ctx.lineWidth = 1;
      ctx.lineCap = 'butt';
      ctx.lineJoin = 'miter';
    }

    } // end wrap-copies loop

    ctx.setTransform(1, 0, 0, 1, 0, 0);
    endRender();
  }, [tiles, tilePositions, showPoliticalColors, showHiddenResources, showAiCivilians, showResources, showTransportNetwork, showArmies, mapMode, nationFillMap,
      isMovementMode, validMoveTargets, isDeployMode, deployableTiles, pendingMoves, nationLabels, disableFogOfWar,
      navyMarkers, seaZones, selectedNavyKey, mapGeometry, tileMap, diplomacyOverlay,
      hideHexGrid, highlightedNationId, classifiedEdges, maxArmyFP, mapDims,
      selectedTileKey, blinkOn, showDiplomacyMarkers, provinceLabels, staticLayer]);

  const scheduleFrame = useCallback(() => {
    if (rafIdRef.current != null) return;
    rafIdRef.current = requestAnimationFrame(() => {
      rafIdRef.current = null;
      render();
    });
  }, [render]);
  // Keep the ref in sync so handlers declared earlier in the component can
  // dispatch to the current scheduler without a TDZ reference.
  scheduleFrameRef.current = scheduleFrame;

  // Schedule a repaint whenever any render-relevant value changes. Includes
  // scale/offset so external state updates (zoom buttons, keyboard, parent-
  // driven pan) still trigger a frame. Also blinkOn for the troop-tier animation.
  useEffect(() => { scheduleFrame(); }, [scheduleFrame, scale, offset, blinkOn]);

  // Blink interval: only runs when the selected tile is a capital with troops.
  useEffect(() => {
    if (blinkIntervalRef.current) {
      clearInterval(blinkIntervalRef.current);
      blinkIntervalRef.current = null;
    }
    const selectedCapitalWithTroops = selectedTileKey
      ? tiles.find(t => `${t.q},${t.r}` === selectedTileKey && t.is_capital && t.army_unit_count > 0)
      : null;
    if (selectedCapitalWithTroops) {
      blinkIntervalRef.current = setInterval(() => {
        setBlinkOn(b => !b);
      }, 500);
    } else {
      setBlinkOn(true);
    }
    return () => {
      if (blinkIntervalRef.current) {
        clearInterval(blinkIntervalRef.current);
        blinkIntervalRef.current = null;
      }
    };
  }, [selectedTileKey, tiles]);

  // Cancel any pending RAF and commit timer on unmount.
  useEffect(() => () => {
    if (rafIdRef.current != null) {
      cancelAnimationFrame(rafIdRef.current);
      rafIdRef.current = null;
    }
    if (gestureCommitTimerRef.current != null) {
      window.clearTimeout(gestureCommitTimerRef.current);
      gestureCommitTimerRef.current = null;
    }
    if (blinkIntervalRef.current) {
      clearInterval(blinkIntervalRef.current);
      blinkIntervalRef.current = null;
    }
  }, []);

  // Re-render when canvas becomes visible after being hidden (display: none → visible)
  // and surface the current viewport size so the tooltip off-screen effect
  // reacts to resizes too. Sets canvas.width/height here (once per resize)
  // instead of on every frame.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const updateSize = () => {
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      setCanvasSize(prev => (prev.w === w && prev.h === h ? prev : { w, h }));
    };
    updateSize();
    const observer = new ResizeObserver(() => {
      updateSize();
      scheduleFrame();
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [scheduleFrame]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      if ((e.target as HTMLElement)?.isContentEditable) return;
      const canvas = canvasRef.current;
      if (!canvas || canvas.clientWidth === 0 || canvas.clientHeight === 0) return;
      const cx = canvas.clientWidth / 2;
      const cy = canvas.clientHeight / 2;
      let delta = 0;
      if (e.key === '+' || e.key === '=') delta = 0.2;
      else if (e.key === '-') delta = -0.2;
      if (delta === 0) return;
      const z = computeZoomRef.current(cx, cy, scaleRef.current, offsetRef.current, scaleRef.current + delta);
      setOffset(z.offset);
      setScale(z.scale);
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Close dropup when clicking outside
  useEffect(() => {
    if (!dropupOpen) return;
    const handleClick = () => setDropupOpen(false);
    window.addEventListener('click', handleClick);
    return () => window.removeEventListener('click', handleClick);
  }, [dropupOpen]);

  const handleMouseDown = (e: React.MouseEvent) => {
    setDragging(true);
    // Use offsetRef so dragStart is relative to the gesture-authoritative
    // offset (which may differ from state mid-commit).
    dragStartRef.current = { x: e.clientX - offsetRef.current.x, y: e.clientY - offsetRef.current.y };
    // A mousedown starts a new gesture; cancel any pending commit so it
    // doesn't fire mid-drag and cause a React render.
    cancelCommitTimer();
    closeNonStickyTooltip();
  };
  /** Order-stable index of each marker within its anchor hex. Determines the
   *  polar offset used to avoid overlap — must be identical in draw and
   *  hit-test to stay consistent. */
  const markerAnchorIndex = useMemo(() => {
    const seen = new Map<string, number>();
    const out = new Map<string, number>();
    for (const m of navyMarkers) {
      const anchorKey = `${m.q},${m.r}`;
      const n = seen.get(anchorKey) ?? 0;
      out.set(navyMarkerKey(m), n);
      seen.set(anchorKey, n + 1);
    }
    return out;
  }, [navyMarkers]);

  const markerAtPoint = (mx: number, my: number): NavyMarker | null => {
    if (navyMarkers.length === 0) return null;
    const r2 = NAVY_MARKER_RADIUS * NAVY_MARKER_RADIUS;
    // Iterate in reverse so markers drawn last (on top) hit-test first.
    for (let i = navyMarkers.length - 1; i >= 0; i--) {
      const m = navyMarkers[i];
      const [px, py] = hexToPixel(m.q, m.r);
      const [dxo, dyo] = navyMarkerOffset(markerAnchorIndex.get(navyMarkerKey(m)) ?? 0);
      const dx = mx - (px + dxo);
      const dy = my - (py + dyo);
      if (dx * dx + dy * dy <= r2) return m;
    }
    return null;
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (dragging) {
      // Write the new offset to the ref and schedule a single frame; do not
      // setState per mousemove (would cascade through App and re-render
      // everything). State commit happens on mouseup.
      const raw = { x: e.clientX - dragStartRef.current.x, y: e.clientY - dragStartRef.current.y };
      const constrained = applyPanConstraints(raw, scaleRef.current);
      offsetRef.current = constrained;
      // Rebase the drag origin against any clamp / wrap adjustment so the
      // next mousemove computes a delta from the constrained position, not
      // the pre-constraint one.
      dragStartRef.current = { x: e.clientX - constrained.x, y: e.clientY - constrained.y };
      scheduleFrame();
      // Dragging dismisses a non-sticky tooltip; sticky persists until click /
      // off-screen.
      closeNonStickyTooltip();
      return;
    }
    if (!canvasRef.current) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const rawMx = (e.clientX - rect.left - offsetRef.current.x) / scaleRef.current;
    const my = (e.clientY - rect.top - offsetRef.current.y) / scaleRef.current;
    // Wrap world x into the primary map copy so hovers on a wrap copy still
    // resolve to the original tile / marker at that q-column.
    const mx = wrapWorldX(rawMx);
    const wrapperX = e.clientX - rect.left;
    const wrapperY = e.clientY - rect.top;

    const marker = markerAtPoint(mx, my);
    if (onNavyMarkerHover) onNavyMarkerHover(marker);

    // A sticky tooltip absorbs hover events: we still report hover for side
    // effects (navy marker outline) but never rearm the open/pin timers.
    if (tooltip?.sticky) {
      if (!marker && onTileHover) {
        const [hq, hr] = wrapHex(...pixelToHex(mx, my));
        onTileHover(tileMap.get(`${hq},${hr}`) || null);
      } else if (marker && onTileHover) {
        onTileHover(null);
      }
      return;
    }

    let key: string;
    let target: { tile?: TileData; marker?: NavyMarker; hexQ: number; hexR: number } | null;
    if (marker) {
      key = `m:${navyMarkerKey(marker)}`;
      target = { marker, hexQ: marker.q, hexR: marker.r };
      if (onTileHover) onTileHover(null);
    } else {
      const [hq, hr] = wrapHex(...pixelToHex(mx, my));
      const tile = tileMap.get(`${hq},${hr}`) || null;
      if (onTileHover) onTileHover(tile);
      if (!tile) {
        hoverKeyRef.current = null;
        hoverTargetRef.current = null;
        if (tooltip && !tooltip.sticky) closeNonStickyTooltip();
        return;
      }
      key = `t:${hq},${hr}`;
      target = { tile, hexQ: hq, hexR: hr };
    }

    hoverPosRef.current = { x: wrapperX, y: wrapperY };

    if (hoverKeyRef.current === key) {
      return;
    }
    hoverKeyRef.current = key;
    hoverTargetRef.current = target;
    if (tooltip && !tooltip.sticky) {
      setTooltip(null);
    }
    armTooltipTimer(key);
  };
  const handleMouseUp = () => {
    if (dragging) {
      // Commit the ref-driven offset back to React state so parents (e.g.
      // App) see the final pan. No state churn during the drag itself.
      commitGestureStateNow();
    }
    setDragging(false);
  };
  const handleMouseLeave = () => {
    if (dragging) commitGestureStateNow();
    setDragging(false);
    closeNonStickyTooltip();
  };

  const handleTouchStart = (e: React.TouchEvent) => {
    e.preventDefault();
    cancelCommitTimer();
    if (e.touches.length === 1) {
      const touch = e.touches[0];
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
      setDragging(true);
      dragStartRef.current = { x: touch.clientX - offsetRef.current.x, y: touch.clientY - offsetRef.current.y };
    } else if (e.touches.length === 2) {
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      lastPinchDistRef.current = Math.sqrt(dx * dx + dy * dy);
      setDragging(false);
    }
  };

  const handleTouchMove = (e: React.TouchEvent) => {
    e.preventDefault();
    if (e.touches.length === 1 && lastTouchRef.current) {
      const touch = e.touches[0];
      const raw = { x: touch.clientX - dragStartRef.current.x, y: touch.clientY - dragStartRef.current.y };
      const constrained = applyPanConstraints(raw, scaleRef.current);
      offsetRef.current = constrained;
      // Rebase drag origin — same invariant as mouse drag.
      dragStartRef.current = { x: touch.clientX - constrained.x, y: touch.clientY - constrained.y };
      scheduleFrame();
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
    } else if (e.touches.length === 2 && lastPinchDistRef.current !== null) {
      const rect = canvasRef.current?.getBoundingClientRect();
      if (!rect) return;
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      const dist = Math.sqrt(dx * dx + dy * dy);
      const scaleFactor = dist / lastPinchDistRef.current;
      const cx = (e.touches[0].clientX + e.touches[1].clientX) / 2 - rect.left;
      const cy = (e.touches[0].clientY + e.touches[1].clientY) / 2 - rect.top;
      zoomAt(cx, cy, scaleRef.current * scaleFactor);
      lastPinchDistRef.current = dist;
    }
  };

  const handleTouchEnd = (e: React.TouchEvent) => {
    e.preventDefault();
    if (e.touches.length === 0) {
      commitGestureStateNow();
      setDragging(false);
      lastTouchRef.current = null;
      lastPinchDistRef.current = null;
    } else if (e.touches.length === 1) {
      const touch = e.touches[0];
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
      lastPinchDistRef.current = null;
      setDragging(true);
      dragStartRef.current = { x: touch.clientX - offsetRef.current.x, y: touch.clientY - offsetRef.current.y };
    }
  };

  // Native wheel listener: React's onWheel is registered as passive in modern
  // Chrome, which means preventDefault() is silently ignored and the page
  // scrolls instead of zooming. Attaching with { passive: false } fixes that
  // and avoids React's event-system overhead on the hot path.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const cx = e.clientX - rect.left;
      const cy = e.clientY - rect.top;
      zoomAtRef.current(cx, cy, scaleRef.current - e.deltaY * 0.001);
      closeNonStickyTooltip();
    };
    canvas.addEventListener('wheel', onWheel, { passive: false });
    return () => canvas.removeEventListener('wheel', onWheel);
    // zoomAt and closeNonStickyTooltip close over refs/stable callbacks — we
    // intentionally only re-bind when scheduleFrame identity changes (i.e.
    // when render deps change).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scheduleFrame]);

  const handleClick = (e: React.MouseEvent) => {
    if (!canvasRef.current) return;
    // Any click dismisses a pinned tooltip (but propagates to selection logic).
    if (tooltip && tooltip.sticky) {
      setTooltip(null);
      clearTooltipTimers();
      hoverKeyRef.current = null;
      hoverTargetRef.current = null;
    }
    const rect = canvasRef.current.getBoundingClientRect();
    const rawMx = (e.clientX - rect.left - offsetRef.current.x) / scaleRef.current;
    const my = (e.clientY - rect.top - offsetRef.current.y) / scaleRef.current;
    // Wrap world x into the primary map copy so clicks on wrap copies
    // resolve to the original tile / marker.
    const mx = wrapWorldX(rawMx);
    const marker = markerAtPoint(mx, my);
    if (marker) {
      if (onNavyMarkerClick) onNavyMarkerClick(marker);
      return;
    }
    if (onNavyMarkerClick) onNavyMarkerClick(null);
    if (onTileClick) {
      // Explicit hit-test for troop-indicator icons before generic hex resolution.
      // Indicators are drawn at (px + HEX_SIZE*0.6, py - HEX_SIZE*0.55) for capital tiles
      // with troops. Clicking within HEX_SIZE*0.7 of an indicator resolves to that capital.
      if (scaleRef.current > 0.6) {
        const hitRadius = HEX_SIZE * 0.7;
        for (const tile of tiles) {
          if (!tile.is_capital || tile.army_unit_count === 0) continue;
          const [px, py] = hexToPixel(tile.q, tile.r);
          const ex = px + HEX_SIZE * 0.6;
          const ey = py - HEX_SIZE * 0.55;
          const dx = mx - ex;
          const dy = my - ey;
          if (dx * dx + dy * dy <= hitRadius * hitRadius) {
            onTileClick(tile);
            return;
          }
        }
      }
      const [hq, hr] = wrapHex(...pixelToHex(mx, my));
      const tile = tileMap.get(`${hq},${hr}`);
      if (tile) onTileClick(tile);
    }
  };

  const controlBtn: React.CSSProperties = {
    padding: '6px 10px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', borderRadius: 4, cursor: 'pointer',
    fontSize: 16, fontFamily: 'Georgia, serif', lineHeight: 1,
  };

  return (
    <div style={{ position: 'relative', width: '100%', height: '100%' }}>
      <canvas
        ref={canvasRef}
        role="img"
        aria-label="Game map"
        style={{ width: '100%', height: '100%', display: 'block', cursor: dragging ? 'grabbing' : isDiplomacyTargetMode ? 'crosshair' : 'grab', touchAction: 'none' }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseLeave}
        onClick={handleClick}
        onTouchStart={handleTouchStart}
        onTouchMove={handleTouchMove}
        onTouchEnd={handleTouchEnd}
      />
      {tooltip && (
        <HexTooltip
          tile={tooltip.tile}
          marker={tooltip.marker}
          screenX={tooltip.screenX}
          screenY={tooltip.screenY}
          sticky={tooltip.sticky}
          showHiddenResources={showHiddenResources}
          governmentTitleByNationId={governmentTitleByNationId}
          modeExtras={tooltip.tile && renderTooltipModeExtras ? renderTooltipModeExtras(tooltip.tile) : null}
        />
      )}
      {/* Map controls */}
      <div style={{ position: 'absolute', bottom: 12, right: 12, display: 'flex', gap: 6, alignItems: 'flex-end' }}>
        {!lockZoom && (
          <>
            <button
              onClick={() => {
                const canvas = canvasRef.current;
                if (!canvas) return;
                zoomAt(canvas.clientWidth / 2, canvas.clientHeight / 2, scale - 0.2);
              }}
              style={controlBtn}
              aria-label="Zoom out"
            >{'\u2212'}</button>
            <button
              onClick={() => {
                const canvas = canvasRef.current;
                if (!canvas) return;
                zoomAt(canvas.clientWidth / 2, canvas.clientHeight / 2, scale + 0.2);
              }}
              style={controlBtn}
              aria-label="Zoom in"
            >+</button>
          </>
        )}

        {/* Map mode dropup */}
        <div style={{ position: 'relative' }}>
          {dropupOpen && (
            <div
              style={{
                position: 'absolute', bottom: '100%', right: 0, marginBottom: 4,
                background: '#2a2518', border: '1px solid #5a5030', borderRadius: 4,
                minWidth: 140, overflow: 'hidden', zIndex: 10,
              }}
              onClick={e => e.stopPropagation()}
            >
              {(['terrain', 'political'] as MapMode[]).map(mode => (
                <button
                  key={mode}
                  onClick={() => { onMapModeChange(mode); setDropupOpen(false); }}
                  style={{
                    display: 'block', width: '100%', textAlign: 'left',
                    padding: '7px 12px', border: 'none', cursor: 'pointer',
                    fontFamily: 'Georgia, serif', fontSize: 13,
                    background: mapMode === mode ? 'rgba(218,165,32,0.15)' : 'transparent',
                    color: mapMode === mode ? '#daa520' : '#e0d8c0',
                  }}
                >
                  {MAP_MODE_LABELS[mode]}
                </button>
              ))}
              {!limitedMapModes && (
                <>
                  <div style={{ height: 1, background: '#5a5030', margin: '2px 0' }} />
                  {(['diplomatic', 'relationship', 'military', 'naval'] as MapMode[]).map(mode => (
                    <button
                      key={mode}
                      onClick={() => { onMapModeChange(mode); setDropupOpen(false); }}
                      style={{
                        display: 'block', width: '100%', textAlign: 'left',
                        padding: '7px 12px', border: 'none', cursor: 'pointer',
                        fontFamily: 'Georgia, serif', fontSize: 13,
                        background: mapMode === mode ? 'rgba(218,165,32,0.15)' : 'transparent',
                        color: mapMode === mode ? '#daa520' : '#e0d8c0',
                      }}
                    >
                      {MAP_MODE_LABELS[mode]}
                    </button>
                  ))}
                </>
              )}
            </div>
          )}
          <button
            onClick={(e) => { e.stopPropagation(); setDropupOpen(o => !o); }}
            style={{ ...controlBtn, padding: '6px 14px', minWidth: 110, textAlign: 'left' }}
          >
            {MAP_MODE_LABELS[mapMode]} {'\u25B4'}
          </button>
        </div>
      </div>
    </div>
  );
}
