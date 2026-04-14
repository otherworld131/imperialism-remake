import { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import type { TileData } from '../wasm';

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
  Indigo: '#4d0080',
};

// Returns a resource icon based on the resource overlay (separate from terrain)
function getResourceIcon(tile: TileData): string | null {
  if (!tile.resource) return null;
  switch (tile.resource) {
    case 'Grain':     return '🌾';
    case 'Fruit':     return '🍎';
    case 'Cotton':    return '🌱';
    case 'Wool':      return '🐑';
    case 'Timber':    return '🪵';
    case 'Livestock': return '🐄';
    case 'Horses':    return '🐴';
    case 'Coal':      return '⛏️';
    case 'Iron':      return '⚒️';
    case 'Gold':      return '💰';
    case 'Gems':      return '💎';
    case 'Oil':       return '🛢️';
    default:          return null;
  }
}

function politicalFill(nationHex: string): string {
  const c = parseInt(nationHex.slice(1), 16);
  const r = (c >> 16) & 0xff, g = (c >> 8) & 0xff, b = c & 0xff;
  return `rgb(${Math.min(255, r + Math.round((255 - r) * 0.45))},${Math.min(255, g + Math.round((255 - g) * 0.45))},${Math.min(255, b + Math.round((255 - b) * 0.45))})`;
}

const ZOOM_CLOSE = 2.0;
const ZOOM_FAR = 0.7;
const POLITICAL_THRESHOLD = 1.1;

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

interface Props {
  tiles: TileData[];
  onTileClick?: (tile: TileData) => void;
  onTileHover?: (tile: TileData | null) => void;
  showHiddenResources?: boolean;
}

export default function HexMap({ tiles, onTileClick, onTileHover, showHiddenResources = false }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [offset, setOffset] = useState({ x: -200, y: -100 });
  const [dragging, setDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [scale, setScale] = useState(ZOOM_FAR);

  const isPolitical = scale < POLITICAL_THRESHOLD;

  const tileMap = useMemo(() => {
    const m = new Map<string, TileData>();
    for (const tile of tiles) {
      m.set(`${tile.q},${tile.r}`, tile);
    }
    return m;
  }, [tiles]);

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

    // ── Pass 1: Fill all hexagons ──
    for (const tile of tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      let color: string;

      if (tile.terrain === 'Sea') {
        color = TERRAIN_COLORS.Sea;
      } else if (isPolitical && tile.owner_color) {
        const nc = NATION_COLORS[tile.owner_color];
        color = nc ? politicalFill(nc) : (TERRAIN_COLORS[tile.terrain] || '#666');
      } else {
        color = TERRAIN_COLORS[tile.terrain] || '#666';
        if (tile.owner_color) {
          const nc = NATION_COLORS[tile.owner_color];
          if (nc) color = tintColor(color, nc, 0.15);
        }
      }

      drawHexagon(ctx, px, py, HEX_SIZE);
      ctx.fillStyle = color;
      ctx.fill();
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
        let borderType: 0 | 1 | 2; // 0=normal, 1=province, 2=country

        if (tile.terrain === 'Sea') {
          // Sea tiles: only draw if neighbor is land (coastline from land side handles it)
          borderType = 0;
        } else if (!neighbor || neighbor.terrain === 'Sea') {
          // Edge of map or coast: country border if owned
          borderType = tile.owner ? 2 : 0;
        } else if (tile.owner !== neighbor.owner) {
          // Different countries
          borderType = 2;
        } else if (tile.owner && tile.province !== neighbor.province) {
          // Same country, different province
          borderType = 1;
        } else {
          // Same country, same province: normal thin edge
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
    if (scale > 0.6 && !isPolitical) {
      const rSize = Math.max(10, HEX_SIZE * 0.7);
      ctx.font = `${rSize}px sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      for (const tile of tiles) {
        if (tile.terrain === 'Sea' || !tile.owner) continue;
        if (tile.is_capital || tile.is_country_capital) continue;
        // Skip hidden resources unless debug toggle is on
        if (tile.resource_hidden && !showHiddenResources) continue;
        const icon = getResourceIcon(tile);
        if (!icon) continue;
        const [px, py] = hexToPixel(tile.q, tile.r);
        ctx.globalAlpha = tile.resource_hidden ? 0.4 : 0.75;
        ctx.fillText(icon, px, py);
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

    // ── Pass 6: Nation name labels (political mode, per-landmass) ──
    if (isPolitical) {
      // Group land tiles by nation
      const nationTiles = new Map<string, Set<string>>();
      for (const tile of tiles) {
        if (tile.terrain === 'Sea' || !tile.owner) continue;
        const key = `${tile.q},${tile.r}`;
        let s = nationTiles.get(tile.owner);
        if (!s) { s = new Set(); nationTiles.set(tile.owner, s); }
        s.add(key);
      }

      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';

      // For each nation, find connected components via BFS
      for (const [name, tileKeys] of nationTiles) {
        const visited = new Set<string>();

        for (const startKey of tileKeys) {
          if (visited.has(startKey)) continue;

          // BFS to find connected component
          const component: string[] = [];
          const queue = [startKey];
          visited.add(startKey);

          while (queue.length > 0) {
            const cur = queue.shift()!;
            component.push(cur);
            const [cq, cr] = cur.split(',').map(Number);
            const nbrs = hexNeighbors(cq, cr);
            for (const [nq, nr] of nbrs) {
              const nk = `${nq},${nr}`;
              if (!visited.has(nk) && tileKeys.has(nk)) {
                visited.add(nk);
                queue.push(nk);
              }
            }
          }

          // Only label landmasses with >= 3 hexes
          if (component.length < 3) continue;

          // Compute centroid
          let sx = 0, sy = 0;
          for (const k of component) {
            const [cq, cr] = k.split(',').map(Number);
            const [px, py] = hexToPixel(cq, cr);
            sx += px; sy += py;
          }
          const cx = sx / component.length;
          const cy = sy / component.length;
          const fontSize = Math.max(12, Math.min(28, Math.sqrt(component.length) * 3));
          ctx.font = `bold ${fontSize}px Georgia, serif`;
          ctx.lineWidth = 3;
          ctx.strokeStyle = 'rgba(0,0,0,0.5)';
          ctx.strokeText(name.toUpperCase(), cx, cy);
          ctx.fillStyle = 'rgba(255,255,255,0.9)';
          ctx.fillText(name.toUpperCase(), cx, cy);
        }
      }
    }

    ctx.restore();
  }, [tiles, offset, scale, isPolitical, showHiddenResources]);

  useEffect(() => { render(); }, [render]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === '+' || e.key === '=') { setScale(s => Math.min(4, s + 0.2)); }
      if (e.key === '-') { setScale(s => Math.max(0.3, s - 0.2)); }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleMouseDown = (e: React.MouseEvent) => {
    setDragging(true);
    setDragStart({ x: e.clientX - offset.x, y: e.clientY - offset.y });
  };
  const handleMouseMove = (e: React.MouseEvent) => {
    if (dragging) {
      setOffset({ x: e.clientX - dragStart.x, y: e.clientY - dragStart.y });
    }
    if (onTileHover && canvasRef.current) {
      const rect = canvasRef.current.getBoundingClientRect();
      const mx = (e.clientX - rect.left - offset.x) / scale;
      const my = (e.clientY - rect.top - offset.y) / scale;
      const [hq, hr] = pixelToHex(mx, my);
      onTileHover(tileMap.get(`${hq},${hr}`) || null);
    }
  };
  const handleMouseUp = () => setDragging(false);

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    setScale(s => Math.max(0.3, Math.min(4, s - e.deltaY * 0.001)));
  };

  const toggleZoom = () => {
    setScale(s => s < POLITICAL_THRESHOLD ? ZOOM_CLOSE : ZOOM_FAR);
  };

  const handleClick = (e: React.MouseEvent) => {
    if (onTileClick && canvasRef.current) {
      const rect = canvasRef.current.getBoundingClientRect();
      const mx = (e.clientX - rect.left - offset.x) / scale;
      const my = (e.clientY - rect.top - offset.y) / scale;
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
        style={{ width: '100%', height: '100%', display: 'block', cursor: dragging ? 'grabbing' : 'grab' }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onWheel={handleWheel}
        onClick={handleClick}
      />
      {/* Map controls */}
      <div style={{ position: 'absolute', bottom: 12, right: 12, display: 'flex', gap: 6, alignItems: 'center' }}>
        <button
          onClick={() => setScale(s => Math.max(0.3, s - 0.2))}
          style={controlBtn}
          aria-label="Zoom out"
        >−</button>
        <button
          onClick={() => setScale(s => Math.min(4, s + 0.2))}
          style={controlBtn}
          aria-label="Zoom in"
        >+</button>
        <button onClick={toggleZoom} style={{ ...controlBtn, padding: '6px 14px' }}>
          {isPolitical ? 'Terrain View' : 'Political View'}
        </button>
      </div>
    </div>
  );
}
