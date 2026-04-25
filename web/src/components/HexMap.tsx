import { useRef, useEffect, useLayoutEffect, useState, useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';
import type { TileData, MapMode, DiplomacyOverlay, MilitaryOverlayEntry, ArmyUnitDetail, ValidMoveTargets, NavyMarker } from '../wasm';
import { computeNationLabels } from '../lib/nationLabels';
import { stitchPolylines, vKey, smoothPolylineAnchored, fbm, type Vec2 } from '../lib/mapGeometry';
import HexTooltip from './HexTooltip';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

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
  Blue: '#3359e6', Gray: '#999', Brown: '#8c5926',
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
  selectedNavyKey?: string | null;
  onNavyMarkerClick?: (marker: NavyMarker | null) => void;
  onNavyMarkerHover?: (marker: NavyMarker | null) => void;
  /** Optional slot to render mode-specific strips (diplomatic / military) inside
   *  the tile tooltip. The parent receives the hovered tile and returns a node. */
  renderTooltipModeExtras?: (tile: TileData) => ReactNode;
  showHiddenResources?: boolean;
  showAiCivilians?: boolean;
  selectedUnit?: ArmyUnitDetail | null;
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
  selectedUnit, pendingMoves = [], validMoveTargets, isMovementMode = false,
  isDeployMode = false, deployableTiles, disableFogOfWar = false,
  organicBorders = true,
  hideHexGrid = false,
  scale: scaleProp, offset: offsetProp, onScaleChange, onOffsetChange,
  highlightedNationId = null,
  navyMarkers = [], selectedNavyKey = null, onNavyMarkerClick, onNavyMarkerHover,
  renderTooltipModeExtras,
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
    if (w > 0) return [((q % w) + w) % w, r];
    return [q, r];
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
    if (nextScale > 4) nextScale = 4;
    const nextOffset = applyPanConstraints(offsetRef.current, nextScale);
    const changedScale = nextScale !== scaleRef.current;
    const changedOffset = nextOffset.x !== offsetRef.current.x || nextOffset.y !== offsetRef.current.y;
    if (!changedScale && !changedOffset) return;
    scaleRef.current = nextScale;
    offsetRef.current = nextOffset;
    if (changedScale) setScale(nextScale);
    if (changedOffset) setOffset(nextOffset);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapDims, canvasSize, applyPanConstraints]);

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
    const clamped = Math.max(fitScale, Math.min(4, newScale));
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

  const showPoliticalColors = mapMode !== 'terrain';

  const tileMap = useMemo(() => {
    const m = new Map<string, TileData>();
    for (const tile of tiles) {
      m.set(`${tile.q},${tile.r}`, tile);
    }
    return m;
  }, [tiles]);

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

  // Memoize nation label BFS — only recompute when tiles change, not on pan/zoom
  // Uses visual_group so incorporated minor nations get their own label
  const nationLabels = useMemo(() => computeNationLabels(tiles), [tiles]);

  // ── Organic coastline + border geometry ─────────────────────────────────
  //
  // Builds smoothed polylines for the land/sea boundary and for political
  // borders so the map doesn't look visibly hexagonal at its silhouette. The
  // hex grid is untouched for gameplay; this is pure presentation. See
  // web/src/lib/mapGeometry.ts. Computed only when organicBorders is on.
  const mapGeometry = useMemo(() => {
    if (!organicBorders) return null;
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
    const wrapNeighbor = (nq: number, nr: number): TileData | undefined => {
      if (mw <= 0) return tileMapLocal.get(`${nq},${nr}`);
      const wq = ((nq % mw) + mw) % mw;
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
    const compTiles: TileData[][] = [];
    for (const tile of landHexes) {
      const tk = `${tile.q},${tile.r}`;
      if (tileComp.has(tk)) continue;
      const vg = tile.visual_group || tile.owner || '';
      const idx = compTiles.length;
      const queue: TileData[] = [tile];
      const members: TileData[] = [];
      tileComp.set(tk, idx);
      while (queue.length > 0) {
        const t = queue.shift()!;
        members.push(t);
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
      compTiles.push(members);
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
    const componentClips: Array<{ path: Path2D; tileKeys: Set<string> }> = [];
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
      const tileKeys = new Set(compTiles[idx].map(t => `${t.q},${t.r}`));
      componentClips.push({ path, tileKeys });
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
      compTiles,
    };
  }, [tiles, organicBorders]);

  /** Pre-classified hex edges (flat [x1,y1,x2,y2,...] arrays). Depends only
   *  on tiles/tileMap — not on scale/offset — so it doesn't recompute during
   *  zoom/pan. In organic-borders mode `country`/`province` go unused (the
   *  smoothed polylines handle those), but they're cheap to produce and the
   *  non-organic fallback in render uses them without a second pass. */
  const classifiedEdges = useMemo(() => {
    const verts = hexVertices(HEX_SIZE);
    const normalEdges: number[] = [];
    const provinceEdges: number[] = [];
    const countryEdges: number[] = [];
    // Wrap-aware neighbor lookup: same rationale as mapGeometry — the east /
    // west map edges are wrap seams, not coastlines, so edge classification
    // must follow the wrapped neighbour (else the non-organic fallback also
    // shows a phantom country stroke at the seam).
    const mw = tiles[0]?.map_width ?? 0;
    const neighborAt = (nq: number, nr: number): TileData | undefined => {
      if (mw <= 0) return tileMap.get(`${nq},${nr}`);
      const wq = ((nq % mw) + mw) % mw;
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
    return { normalEdges, provinceEdges, countryEdges };
  }, [tiles, tileMap]);

  /** Max army firepower across all capitals — used to normalize the per-capital
   *  strength-bar width. Previously re-scanned every frame. */
  const maxArmyFP = useMemo(() => {
    let m = 0;
    for (const tile of tiles) {
      if (tile.is_capital && tile.army_firepower > m) m = tile.army_firepower;
    }
    return m < 1 ? 1 : m;
  }, [tiles]);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

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

    for (let k = kMin; k <= kMax; k++) {
      ctx.setTransform(scale, 0, 0, scale, offset.x + k * periodScreen, offset.y);

    if (mapGeometry) {
      // Sea backdrop is the canvas-wide fill above the loop. Drawing the
      // per-copy seaBox here would overpaint the previous copy's land where
      // their world rects overlap on the canvas (wrap-copy land vanishes).

      // ── Pass 1: Land fills, clipped PER visual-group component so a
      // nation's color can't leak past its smoothed border onto the neighbour.
      for (let i = 0; i < mapGeometry.componentClips.length; i++) {
        const comp = mapGeometry.componentClips[i];
        const compTilesArr = mapGeometry.compTiles[i];
        // Fatten pass: fill the full component polygon with a representative
        // color so the boundary anti-aliasing zone ends up land-tinted
        // instead of sea-tinted. Without this, the AA-blended pixels at the
        // smoothed edge let the sea background bleed through as a blue
        // sliver below the stroke.
        const first = compTilesArr[0];
        if (first) {
          ctx.fillStyle = tileFillColor(first);
          ctx.fill(comp.path, 'evenodd');
        }
        ctx.save();
        ctx.clip(comp.path, 'evenodd');
        for (const tile of compTilesArr) {
          const [px, py] = hexToPixel(tile.q, tile.r);
          drawHexagon(ctx, px, py, HEX_SIZE);
          ctx.fillStyle = tileFillColor(tile);
          ctx.fill();
        }
        ctx.restore();
      }
    } else {
      // ── Original non-organic rendering: per-hex fills for every tile ──
      for (const tile of tiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.fillStyle = tileFillColor(tile);
        ctx.fill();
      }
    }

    // Fog of war — applied per-hex (land and sea) in both modes.
    if (!disableFogOfWar) {
      ctx.fillStyle = 'rgba(128, 128, 128, 0.35)';
      for (const tile of tiles) {
        if (tile.visible) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.fill();
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
      if (!hideHexGrid) {
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

        ctx.strokeStyle = 'rgba(20,15,10,0.5)';
        ctx.lineWidth = 1.5;
        for (const loop of mapGeometry.provincePolylinesClosed) strokePolyline(loop, true);
        for (const line of mapGeometry.provincePolylinesOpen) strokePolyline(line, false);

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
        ctx.strokeStyle = 'rgba(20,15,10,0.5)';
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        for (let i = 0; i < provinceEdges.length; i += 4) {
          ctx.moveTo(provinceEdges[i], provinceEdges[i + 1]);
          ctx.lineTo(provinceEdges[i + 2], provinceEdges[i + 3]);
        }
        ctx.stroke();

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
          const compTilesArr = mapGeometry.compTiles[i];
          if (!compTilesArr.some(t => t.nation_id === highlightedNationId)) continue;
          ctx.stroke(mapGeometry.componentClips[i].path);
        }
      } else {
        ctx.strokeStyle = PREVIEW_HIGHLIGHT_COLOR;
        ctx.lineWidth = PREVIEW_HIGHLIGHT_WIDTH;
        ctx.lineCap = 'butt';
        ctx.lineJoin = 'miter';
        for (const tile of tiles) {
          if (tile.terrain === 'Sea') continue;
          if (tile.nation_id !== highlightedNationId) continue;
          const [px, py] = hexToPixel(tile.q, tile.r);
          drawHexagon(ctx, px, py, HEX_SIZE * 0.95);
          ctx.stroke();
        }
      }
    }

    // ── Pass 3: Capitals ──
    for (const tile of tiles) {
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
    if (scale > 0.6 && mapMode === 'terrain') {
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
      for (const tile of tiles) {
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
      for (const tile of tiles) {
        if (tile.terrain === 'Sea') continue;
        if (!tile.has_railroad && !tile.has_depot && !tile.has_port && !tile.has_fort) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);

        if (tile.has_railroad) {
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
        if (tile.has_depot) {
          ctx.fillStyle = 'rgba(139,90,43,0.9)';
          const ds = HEX_SIZE * 0.2;
          ctx.fillRect(px - ds + HEX_SIZE * 0.3, py - ds, ds * 2, ds * 2);
          ctx.strokeStyle = 'rgba(0,0,0,0.6)';
          ctx.lineWidth = 0.8;
          ctx.strokeRect(px - ds + HEX_SIZE * 0.3, py - ds, ds * 2, ds * 2);
        }
        if (tile.has_port) {
          ctx.lineWidth = 1;
          ctx.strokeStyle = 'rgba(0,0,0,0.6)';
          ctx.strokeText('\u2693', px - HEX_SIZE * 0.3, py);
          ctx.fillStyle = 'rgba(70,130,200,0.9)';
          ctx.fillText('\u2693', px - HEX_SIZE * 0.3, py);
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
    if (mapMode === 'diplomatic' && diplomacyOverlay && scale > 0.6) {
      const diploByNation = new Map<string, typeof diplomacyOverlay.relations[0]>();
      for (const rel of diplomacyOverlay.relations) {
        diploByNation.set(rel.nation_name, rel);
      }

      const badgeSize = Math.max(9, HEX_SIZE * 0.35);
      const badgeR = badgeSize * 0.65;

      for (const tile of tiles) {
        if (!tile.is_country_capital || tile.terrain === 'Sea') continue;
        if (!tile.owner) continue;
        const rel = diploByNation.get(tile.owner);
        if (!rel) continue;

        if (!rel.has_consulate && !rel.has_embassy) continue;

        const [px, py] = hexToPixel(tile.q, tile.r);
        const iy = py + HEX_SIZE * 0.55;

        // Draw badge circle + letter
        const letter = rel.has_embassy ? 'E' : 'C';
        const bgColor = rel.has_embassy ? 'rgba(30,80,160,0.85)' : 'rgba(0,150,136,0.85)';

        ctx.beginPath();
        ctx.arc(px, iy, badgeR, 0, Math.PI * 2);
        ctx.fillStyle = bgColor;
        ctx.fill();
        ctx.strokeStyle = 'rgba(218,165,32,0.9)';
        ctx.lineWidth = 1.2;
        ctx.stroke();

        ctx.font = `bold ${badgeSize * 0.7}px Georgia, serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = '#fff';
        ctx.fillText(letter, px, iy);
      }
    }

    // ── Pass 6: Nation name labels (all non-terrain modes) ──
    if (showPoliticalColors) {
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

    // ── Pass 7: Army strength bar at capitals (navy is surfaced via the
    //             navy markers on the adjacent sea hex, so no naval bar here).
    if (scale > 0.8) {
      const maxBarWidth = HEX_SIZE * 0.8;

      // Font + text alignment are uniform across every capital badge — set
      // once outside the loop instead of per tile.
      ctx.font = '7px sans-serif';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';

      for (const tile of tiles) {
        if (tile.terrain === 'Sea') continue;
        if (!tile.is_capital) continue;
        if (tile.army_unit_count === 0) continue;

        const [px, py] = hexToPixel(tile.q, tile.r);
        const barWidth = Math.max(3, (tile.army_firepower / maxArmyFP) * maxBarWidth);
        const barX = px - barWidth / 2;
        const barY = py + HEX_SIZE * 0.45;

        ctx.fillStyle = '#8b0000';
        ctx.fillRect(barX, barY, barWidth, 2.5);
        ctx.strokeStyle = 'rgba(0,0,0,0.6)';
        ctx.lineWidth = 0.5;
        ctx.strokeRect(barX, barY, barWidth, 2.5);

        ctx.fillStyle = 'rgba(255,255,255,0.9)';
        ctx.fillText(`x${tile.army_unit_count}`, barX + barWidth + 2, barY + 1.5);
      }
    }

    // ── Pass 8: Civilian emoji icons on hex tiles ──────────────
    if (scale > 0.7) {
      const civFontSize = Math.max(6, HEX_SIZE * 0.55);
      ctx.font = `${civFontSize}px sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      for (const tile of tiles) {
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

      for (const tile of tiles) {
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
      for (const tile of tiles) {
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

      ctx.setLineDash([4, 3]);
      ctx.lineWidth = 1.5;
      for (const move of pendingMoves) {
        const from = capitalPositions.get(move.source_province_id);
        const to = capitalPositions.get(move.dest_province_id);
        if (!from || !to) continue;

        ctx.strokeStyle = 'rgba(255, 200, 0, 0.8)';
        ctx.beginPath();
        ctx.moveTo(from[0], from[1]);
        ctx.lineTo(to[0], to[1]);
        ctx.stroke();

        // Arrowhead
        const angle = Math.atan2(to[1] - from[1], to[0] - from[0]);
        const arrowLen = 5;
        ctx.setLineDash([]);
        ctx.beginPath();
        ctx.moveTo(to[0], to[1]);
        ctx.lineTo(to[0] - arrowLen * Math.cos(angle - 0.4), to[1] - arrowLen * Math.sin(angle - 0.4));
        ctx.moveTo(to[0], to[1]);
        ctx.lineTo(to[0] - arrowLen * Math.cos(angle + 0.4), to[1] - arrowLen * Math.sin(angle + 0.4));
        ctx.stroke();
        ctx.setLineDash([4, 3]);
      }
      ctx.setLineDash([]);
    }

    } // end wrap-copies loop

    ctx.setTransform(1, 0, 0, 1, 0, 0);
  }, [tiles, showPoliticalColors, showHiddenResources, showAiCivilians, mapMode, nationFillMap,
      isMovementMode, validMoveTargets, isDeployMode, deployableTiles, pendingMoves, nationLabels, disableFogOfWar,
      navyMarkers, selectedNavyKey, mapGeometry, tileMap, diplomacyOverlay,
      hideHexGrid, highlightedNationId, classifiedEdges, maxArmyFP, mapDims]);

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
  // driven pan) still trigger a frame.
  useEffect(() => { scheduleFrame(); }, [scheduleFrame, scale, offset]);

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
        style={{ width: '100%', height: '100%', display: 'block', cursor: dragging ? 'grabbing' : 'grab', touchAction: 'none' }}
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
          modeExtras={tooltip.tile && renderTooltipModeExtras ? renderTooltipModeExtras(tooltip.tile) : null}
        />
      )}
      {/* Map controls */}
      <div style={{ position: 'absolute', bottom: 12, right: 12, display: 'flex', gap: 6, alignItems: 'flex-end' }}>
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
