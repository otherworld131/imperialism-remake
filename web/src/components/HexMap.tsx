import { useRef, useEffect, useLayoutEffect, useState, useCallback, useMemo } from 'react';
import type { ReactNode } from 'react';
import type { TileData, MapMode, DiplomacyOverlay, MilitaryOverlayEntry, ArmyUnitDetail, ValidMoveTargets, NavyMarker } from '../wasm';
import { computeNationLabels } from '../lib/nationLabels';
import HexTooltip from './HexTooltip';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

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
  scale: scaleProp, offset: offsetProp, onScaleChange, onOffsetChange,
  highlightedNationId = null,
  navyMarkers = [], selectedNavyKey = null, onNavyMarkerClick, onNavyMarkerHover,
  renderTooltipModeExtras,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Use props if provided (controlled mode), otherwise use local state (uncontrolled)
  const [localOffset, setLocalOffset] = useState({ x: -200, y: -100 });
  const [dragging, setDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [localScale, setLocalScale] = useState(0.7);
  const [dropupOpen, setDropupOpen] = useState(false);
  const lastTouchRef = useRef<{ x: number; y: number } | null>(null);
  const lastPinchDistRef = useRef<number | null>(null);
  const scaleRef = useRef(scaleProp ?? 0.7);
  const offsetRef = useRef(offsetProp ?? { x: -200, y: -100 });

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

  // If a pinned tooltip's hex has been panned/zoomed (or the canvas resized)
  // off-screen, dismiss it.
  useEffect(() => {
    if (!tooltip) return;
    const cw = canvasSize.w;
    const ch = canvasSize.h;
    if (cw === 0 || ch === 0) return;
    const [px, py] = hexToPixel(tooltip.hexQ, tooltip.hexR);
    const sx = px * scale + offset.x;
    const sy = py * scale + offset.y;
    const pad = HEX_SIZE * scale;
    if (sx < -pad || sy < -pad || sx > cw + pad || sy > ch + pad) {
      setTooltip(null);
    }
  }, [tooltip, scale, offset, canvasSize]);

  /** Pure zoom-to-point math: returns clamped new scale and adjusted offset. */
  const computeZoom = (cx: number, cy: number, oldScale: number, oldOffset: { x: number; y: number }, newScale: number) => {
    const clamped = Math.max(0.3, Math.min(4, newScale));
    const ratio = clamped / oldScale;
    return {
      scale: clamped,
      offset: { x: cx - (cx - oldOffset.x) * ratio, y: cy - (cy - oldOffset.y) * ratio },
    };
  };

  /** Zoom toward a screen-space point, adjusting offset so that point stays fixed. */
  const zoomAt = (cx: number, cy: number, newScale: number) => {
    const z = computeZoom(cx, cy, scale, offset, newScale);
    setOffset(z.offset);
    setScale(z.scale);
  };

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

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const verts = hexVertices(HEX_SIZE);

    ctx.save();
    ctx.translate(offset.x, offset.y);
    ctx.scale(scale, scale);

    // Helper: pick the right political fill based on incorporated status
    const pickPoliticalColor = (tile: TileData): string => {
      if (!tile.owner_color) return TERRAIN_COLORS[tile.terrain] || '#666';
      const nc = NATION_COLORS[tile.owner_color];
      if (!nc) return TERRAIN_COLORS[tile.terrain] || '#666';
      return tile.is_incorporated_minor ? incorporatedFill(nc) : politicalFill(nc);
    };

    // ── Pass 1: Fill all hexagons ──
    for (const tile of tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      let color: string;

      if (tile.terrain === 'Sea') {
        color = TERRAIN_COLORS.Sea;
      } else if (mapMode === 'terrain') {
        // Terrain mode: base terrain + subtle nation tint
        color = TERRAIN_COLORS[tile.terrain] || '#666';
        if (tile.owner_color) {
          const nc = NATION_COLORS[tile.owner_color];
          if (nc) color = tintColor(color, nc, tile.is_incorporated_minor ? 0.10 : 0.15);
        }
      } else if (mapMode === 'diplomatic' || mapMode === 'relationship') {
        // Overlay modes: use nationFillMap colors, fall back to political fill
        const overlayColor = tile.owner ? nationFillMap.get(tile.owner) : null;
        if (overlayColor) {
          color = overlayColor;
        } else {
          color = pickPoliticalColor(tile);
        }
      } else if (mapMode === 'military' || mapMode === 'naval') {
        // Military/Naval: political base with strength tint from nationFillMap
        const overlayColor = tile.owner ? nationFillMap.get(tile.owner) : null;
        if (overlayColor) {
          color = overlayColor;
        } else {
          color = pickPoliticalColor(tile);
        }
      } else {
        // Political mode
        color = pickPoliticalColor(tile);
      }

      drawHexagon(ctx, px, py, HEX_SIZE);
      ctx.fillStyle = color;
      ctx.fill();

      // Fog of war overlay: gray out non-visible tiles
      if (!tile.visible && !disableFogOfWar) {
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.fillStyle = 'rgba(128, 128, 128, 0.35)';
        ctx.fill();
      }
    }

    // ── Pass 2: Draw each hex side with appropriate thickness ──
    // Collect segments into 3 buckets by thickness
    const normalEdges: number[] = [];   // x1,y1,x2,y2 flat array
    const provinceEdges: number[] = [];
    const countryEdges: number[] = [];

    for (const tile of tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      const neighbors = hexNeighbors(tile.q, tile.r);

      for (let i = 0; i < 6; i++) {
        const [nq, nr] = neighbors[i];
        const neighbor = tileMap.get(`${nq},${nr}`);

        // Determine border type for THIS side
        // Use visual_group for border grouping: incorporated minor provinces
        // keep separate country-level borders from their overlord GP
        let borderType: 0 | 1 | 2; // 0=normal, 1=province, 2=country
        const tileVG = tile.visual_group || tile.owner;
        const neighborVG = neighbor ? (neighbor.visual_group || neighbor.owner) : '';

        if (tile.terrain === 'Sea') {
          // Sea tiles: only draw if neighbor is land (coastline from land side handles it)
          borderType = 0;
        } else if (!neighbor || neighbor.terrain === 'Sea') {
          // Edge of map or coast: country border if owned
          borderType = tile.owner ? 2 : 0;
        } else if (tileVG !== neighborVG) {
          // Different visual groups (different nations, or GP vs incorporated minor)
          borderType = 2;
        } else if (tile.owner && tile.province !== neighbor.province) {
          // Same visual group, different province
          borderType = 1;
        } else {
          // Same visual group, same province: normal thin edge
          borderType = 0;
        }

        // Get the two vertices of this edge
        const v1 = verts[i];
        const v2 = verts[(i + 1) % 6];
        const x1 = px + v1[0], y1 = py + v1[1];
        const x2 = px + v2[0], y2 = py + v2[1];

        if (borderType === 2) {
          countryEdges.push(x1, y1, x2, y2);
        } else if (borderType === 1) {
          provinceEdges.push(x1, y1, x2, y2);
        } else {
          normalEdges.push(x1, y1, x2, y2);
        }
      }
    }

    // Draw normal edges (very thin, subtle)
    ctx.strokeStyle = 'rgba(0,0,0,0.08)';
    ctx.lineWidth = 0.5;
    ctx.lineCap = 'butt';
    ctx.beginPath();
    for (let i = 0; i < normalEdges.length; i += 4) {
      ctx.moveTo(normalEdges[i], normalEdges[i + 1]);
      ctx.lineTo(normalEdges[i + 2], normalEdges[i + 3]);
    }
    ctx.stroke();

    // Draw province edges (medium)
    ctx.strokeStyle = 'rgba(20,15,10,0.5)';
    ctx.lineWidth = 1.5;
    ctx.lineCap = 'butt';
    ctx.beginPath();
    for (let i = 0; i < provinceEdges.length; i += 4) {
      ctx.moveTo(provinceEdges[i], provinceEdges[i + 1]);
      ctx.lineTo(provinceEdges[i + 2], provinceEdges[i + 3]);
    }
    ctx.stroke();

    // Draw country edges (thick, on top)
    ctx.strokeStyle = 'rgba(10,5,0,0.9)';
    ctx.lineWidth = 3.5;
    ctx.lineCap = 'butt';
    ctx.beginPath();
    for (let i = 0; i < countryEdges.length; i += 4) {
      ctx.moveTo(countryEdges[i], countryEdges[i + 1]);
      ctx.lineTo(countryEdges[i + 2], countryEdges[i + 3]);
    }
    ctx.stroke();

    // ── Pass 2.5: Highlight selected nation's tiles (setup preview) ──
    if (highlightedNationId != null) {
      ctx.strokeStyle = '#ffd700';
      ctx.lineWidth = 2.5;
      ctx.lineCap = 'butt';
      ctx.lineJoin = 'miter';
      for (const tile of tiles) {
        if (tile.nation_id !== highlightedNationId) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE * 0.95);
        ctx.stroke();
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
      for (const tile of tiles) {
        if (tile.terrain === 'Sea' || !tile.owner) continue;
        if (tile.is_capital || tile.is_country_capital) continue;
        // Skip hidden resources unless debug toggle is on
        if (tile.resource_hidden && !showHiddenResources) continue;
        const icon = getResourceIcon(tile);
        if (!icon) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);

        ctx.font = `${rSize}px sans-serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.globalAlpha = tile.resource_hidden ? 0.4 : 0.75;
        ctx.fillText(icon, px, py);
        ctx.globalAlpha = 1.0;

        // Improvement-level badge (e.g. "2/3"), gold when fully improved
        if (tile.improvement_level > 0 && tile.max_improvement_level > 0) {
          ctx.font = `bold ${badgeFont}px sans-serif`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          const fully = tile.improvement_level >= tile.max_improvement_level;
          const text = `${tile.improvement_level}/${tile.max_improvement_level}`;
          const bx = px + HEX_SIZE * 0.5;
          const by = py + HEX_SIZE * 0.55;
          ctx.lineWidth = 3;
          ctx.strokeStyle = 'rgba(0,0,0,0.85)';
          ctx.strokeText(text, bx, by);
          ctx.fillStyle = fully ? '#ffd700' : '#fff';
          ctx.fillText(text, bx, by);
        }
      }
      ctx.globalAlpha = 1.0;
    }

    // ── Pass 5: Infrastructure icons ──
    if (scale > 0.8) {
      const iconSize = Math.max(8, HEX_SIZE * 0.5);
      for (const tile of tiles) {
        if (tile.terrain === 'Sea') continue;
        if (!tile.has_railroad && !tile.has_depot && !tile.has_port && !tile.has_fort) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);
        ctx.font = `${iconSize}px sans-serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

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
      let maxArmyFP = 0;
      for (const tile of tiles) {
        if (tile.is_capital && tile.army_firepower > maxArmyFP) maxArmyFP = tile.army_firepower;
      }
      if (maxArmyFP < 1) maxArmyFP = 1;

      const maxBarWidth = HEX_SIZE * 0.8;

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

        ctx.font = '7px sans-serif';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
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

    ctx.restore();
  }, [tiles, offset, scale, showPoliticalColors, showHiddenResources, showAiCivilians, mapMode, nationFillMap,
      isMovementMode, validMoveTargets, isDeployMode, deployableTiles, pendingMoves, nationLabels, disableFogOfWar,
      navyMarkers, selectedNavyKey]);

  useEffect(() => { render(); }, [render]);

  // Re-render when canvas becomes visible after being hidden (display: none → visible)
  // and surface the current viewport size so the tooltip off-screen effect
  // reacts to resizes too.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const updateSize = () => {
      setCanvasSize(prev => {
        const w = canvas.clientWidth;
        const h = canvas.clientHeight;
        if (prev.w === w && prev.h === h) return prev;
        return { w, h };
      });
    };
    updateSize();
    const observer = new ResizeObserver(() => {
      updateSize();
      render();
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [render]);

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
      const z = computeZoom(cx, cy, scaleRef.current, offsetRef.current, scaleRef.current + delta);
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
    setDragStart({ x: e.clientX - offset.x, y: e.clientY - offset.y });
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
      setOffset({ x: e.clientX - dragStart.x, y: e.clientY - dragStart.y });
      // Dragging dismisses a non-sticky tooltip; sticky persists until click /
      // off-screen.
      closeNonStickyTooltip();
      return;
    }
    if (!canvasRef.current) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const mx = (e.clientX - rect.left - offset.x) / scale;
    const my = (e.clientY - rect.top - offset.y) / scale;
    const wrapperX = e.clientX - rect.left;
    const wrapperY = e.clientY - rect.top;

    const marker = markerAtPoint(mx, my);
    if (onNavyMarkerHover) onNavyMarkerHover(marker);

    // A sticky tooltip absorbs hover events: we still report hover for side
    // effects (navy marker outline) but never rearm the open/pin timers.
    if (tooltip?.sticky) {
      if (!marker && onTileHover) {
        const [hq, hr] = pixelToHex(mx, my);
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
      const [hq, hr] = pixelToHex(mx, my);
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
  const handleMouseUp = () => setDragging(false);
  const handleMouseLeave = () => {
    setDragging(false);
    closeNonStickyTooltip();
  };

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    zoomAt(cx, cy, scale - e.deltaY * 0.001);
    closeNonStickyTooltip();
  };

  const handleTouchStart = (e: React.TouchEvent) => {
    e.preventDefault();
    if (e.touches.length === 1) {
      const touch = e.touches[0];
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
      setDragging(true);
      setDragStart({ x: touch.clientX - offset.x, y: touch.clientY - offset.y });
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
      setOffset({ x: touch.clientX - dragStart.x, y: touch.clientY - dragStart.y });
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
      zoomAt(cx, cy, scale * scaleFactor);
      lastPinchDistRef.current = dist;
    }
  };

  const handleTouchEnd = (e: React.TouchEvent) => {
    e.preventDefault();
    if (e.touches.length === 0) {
      setDragging(false);
      lastTouchRef.current = null;
      lastPinchDistRef.current = null;
    } else if (e.touches.length === 1) {
      const touch = e.touches[0];
      lastTouchRef.current = { x: touch.clientX, y: touch.clientY };
      lastPinchDistRef.current = null;
      setDragging(true);
      setDragStart({ x: touch.clientX - offset.x, y: touch.clientY - offset.y });
    }
  };

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
    const mx = (e.clientX - rect.left - offset.x) / scale;
    const my = (e.clientY - rect.top - offset.y) / scale;
    const marker = markerAtPoint(mx, my);
    if (marker) {
      if (onNavyMarkerClick) onNavyMarkerClick(marker);
      return;
    }
    if (onNavyMarkerClick) onNavyMarkerClick(null);
    if (onTileClick) {
      const [hq, hr] = pixelToHex(mx, my);
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
        onWheel={handleWheel}
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
