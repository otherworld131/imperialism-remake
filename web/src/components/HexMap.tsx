import { useRef, useEffect, useState, useCallback } from 'react';
import type { TileData } from '../wasm';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

const TERRAIN_COLORS: Record<string, string> = {
  Farm:           '#7aba3a',
  HardwoodForest: '#2d7a2d',
  ScrubForest:    '#5a9a4a',
  FertileHills:   '#8db35a',
  BarrenHills:    '#9e8a6a',
  Mountain:       '#8a7d70',
  Sea:            '#6fa8d6',
  DryPlains:      '#d4c98a',
  Plantation:     '#5aad3e',
  OpenRange:      '#c2cc7a',
  HorseRanch:     '#b8a85a',
  Orchard:        '#9acc4a',
  Swamp:          '#5a7a5a',
  Desert:         '#ddd0a0',
  Tundra:         '#c5d0db',
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

// Lighter, semi-transparent versions for political map fills
function politicalFill(nationHex: string): string {
  const c = parseInt(nationHex.slice(1), 16);
  const r = (c >> 16) & 0xff, g = (c >> 8) & 0xff, b = c & 0xff;
  // Lighten and desaturate slightly
  const lr = Math.min(255, r + Math.round((255 - r) * 0.45));
  const lg = Math.min(255, g + Math.round((255 - g) * 0.45));
  const lb = Math.min(255, b + Math.round((255 - b) * 0.45));
  return `rgb(${lr},${lg},${lb})`;
}

const ZOOM_CLOSE = 2.0;
const ZOOM_FAR = 0.7;
const POLITICAL_THRESHOLD = 1.1; // Below this scale → political map

function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
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

function hexNeighbors(q: number, r: number): [number, number][] {
  return [
    [q + 1, r], [q + 1, r - 1], [q, r - 1],
    [q - 1, r], [q - 1, r + 1], [q, r + 1],
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
}

export default function HexMap({ tiles, onTileClick, onTileHover }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [offset, setOffset] = useState({ x: -200, y: -100 });
  const [dragging, setDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [scale, setScale] = useState(ZOOM_FAR);

  const mapWidth = tiles.length > 0 ? (tiles[0].map_width || 60) : 60;
  const mapPixelWidth = HEX_SIZE * SQRT3 * mapWidth;
  const isPolitical = scale < POLITICAL_THRESHOLD;

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const tileMap = new Map<string, TileData>();
    for (const tile of tiles) {
      tileMap.set(`${tile.q},${tile.r}`, tile);
    }

    // Compute nation centroids for label placement
    const nationTiles = new Map<string, { sx: number; sy: number; count: number; color: string }>();

    const wraps = [-1, 0, 1];

    for (const wrapOffset of wraps) {
      ctx.save();
      ctx.translate(offset.x + wrapOffset * mapPixelWidth * scale, offset.y);
      ctx.scale(scale, scale);

      // ── Pass 1: Fill hexagons ──
      for (const tile of tiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        let color: string;

        if (tile.terrain === 'Sea') {
          color = TERRAIN_COLORS.Sea;
        } else if (isPolitical && tile.owner_color) {
          // Political mode: fill with light nation color
          const nc = NATION_COLORS[tile.owner_color];
          color = nc ? politicalFill(nc) : (TERRAIN_COLORS[tile.terrain] || '#666');
        } else {
          // Geographical mode: terrain color with subtle nation tint
          color = TERRAIN_COLORS[tile.terrain] || '#666';
          if (tile.owner_color) {
            const nc = NATION_COLORS[tile.owner_color];
            if (nc) color = tintColor(color, nc, 0.15);
          }
        }

        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.fillStyle = color;
        ctx.fill();

        // Accumulate centroid for nation labels (only for wrap 0)
        if (wrapOffset === 0 && tile.owner && tile.terrain !== 'Sea') {
          const key = tile.owner;
          const entry = nationTiles.get(key);
          if (entry) {
            entry.sx += px;
            entry.sy += py;
            entry.count++;
          } else {
            nationTiles.set(key, {
              sx: px, sy: py, count: 1,
              color: NATION_COLORS[tile.owner_color] || '#333',
            });
          }
        }
      }

      // ── Pass 2: Border hexagon outlines ──
      // Instead of drawing individual edge segments (which leave gaps),
      // draw the full hexagon outline for tiles that have ANY border edge.
      // Country-border hexes get a thick outline, province-border hexes get thinner.

      // First: identify which hexes need which border type
      const countryBorderHexes: TileData[] = [];
      const provinceBorderHexes: TileData[] = [];

      for (const tile of tiles) {
        if (tile.terrain === 'Sea') continue;
        const neighbors = hexNeighbors(tile.q, tile.r);
        let hasCountryBorder = false;
        let hasProvinceBorder = false;

        for (const [nq, nr] of neighbors) {
          const neighbor = tileMap.get(`${nq},${nr}`);
          if (!neighbor || neighbor.terrain === 'Sea') {
            if (tile.owner) hasCountryBorder = true;
          } else if (tile.owner !== neighbor.owner) {
            hasCountryBorder = true;
          } else if (tile.owner && tile.province !== neighbor.province) {
            hasProvinceBorder = true;
          }
        }

        if (hasCountryBorder) countryBorderHexes.push(tile);
        else if (hasProvinceBorder) provinceBorderHexes.push(tile);
      }

      // Draw province borders: outline the full hex
      ctx.strokeStyle = 'rgba(30,20,10,0.35)';
      ctx.lineWidth = 1.2;
      for (const tile of provinceBorderHexes) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.stroke();
      }

      // Draw country borders: outline the full hex with thick line
      ctx.strokeStyle = 'rgba(10,5,0,0.85)';
      ctx.lineWidth = 3;
      for (const tile of countryBorderHexes) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        drawHexagon(ctx, px, py, HEX_SIZE);
        ctx.stroke();
      }

      // ── Pass 3: Capitals ──
      for (const tile of tiles) {
        if (!tile.is_capital || tile.terrain === 'Sea') continue;
        const [px, py] = hexToPixel(tile.q, tile.r);

        if (tile.is_country_capital) {
          // Country capital: large gold star with black outline
          const sz = Math.max(15, HEX_SIZE * 0.9);
          ctx.font = `bold ${sz}px serif`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          // Black outline via strokeText
          ctx.lineWidth = 2.5;
          ctx.strokeStyle = 'rgba(0,0,0,0.8)';
          ctx.strokeText('\u2605', px, py);
          ctx.fillStyle = '#ffd700';
          ctx.fillText('\u2605', px, py);
        } else {
          // Province capital: small white dot
          ctx.beginPath();
          ctx.arc(px, py, 2.5, 0, Math.PI * 2);
          ctx.fillStyle = 'rgba(255,255,255,0.7)';
          ctx.fill();
          ctx.strokeStyle = 'rgba(0,0,0,0.4)';
          ctx.lineWidth = 0.8;
          ctx.stroke();
        }
      }

      // ── Pass 4: Nation name labels (political mode only, wrap 0) ──
      if (isPolitical && wrapOffset === 0) {
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        for (const [name, info] of nationTiles) {
          const cx = info.sx / info.count;
          const cy = info.sy / info.count;
          // Scale font with territory size
          const fontSize = Math.max(12, Math.min(28, Math.sqrt(info.count) * 3));
          ctx.font = `bold ${fontSize}px Georgia, serif`;
          // Dark outline
          ctx.lineWidth = 3;
          ctx.strokeStyle = 'rgba(0,0,0,0.5)';
          ctx.strokeText(name.toUpperCase(), cx, cy);
          // White fill
          ctx.fillStyle = 'rgba(255,255,255,0.9)';
          ctx.fillText(name.toUpperCase(), cx, cy);
        }
      }

      ctx.restore();
    }
  }, [tiles, offset, scale, mapPixelWidth, isPolitical]);

  useEffect(() => { render(); }, [render]);

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
      let closest: TileData | null = null;
      let minDist = Infinity;
      for (const tile of tiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        for (const w of [-1, 0, 1]) {
          const d = Math.hypot(mx - px - w * mapPixelWidth, my - py);
          if (d < HEX_SIZE && d < minDist) { minDist = d; closest = tile; }
        }
      }
      onTileHover(closest);
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
      let closest: TileData | null = null;
      let minDist = Infinity;
      for (const tile of tiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        for (const w of [-1, 0, 1]) {
          const d = Math.hypot(mx - px - w * mapPixelWidth, my - py);
          if (d < HEX_SIZE && d < minDist) { minDist = d; closest = tile; }
        }
      }
      if (closest) onTileClick(closest);
    }
  };

  return (
    <div style={{ position: 'relative', width: '100%', height: '100%' }}>
      <canvas
        ref={canvasRef}
        style={{ width: '100%', height: '100%', cursor: dragging ? 'grabbing' : 'grab' }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onWheel={handleWheel}
        onClick={handleClick}
      />
      <button
        onClick={toggleZoom}
        style={{
          position: 'absolute', bottom: 12, right: 12,
          padding: '6px 14px', background: '#3a3520', color: '#e0d8c0',
          border: '1px solid #5a5030', borderRadius: 4, cursor: 'pointer',
          fontSize: 13, fontFamily: 'Georgia, serif',
        }}
      >
        {isPolitical ? '🔍 Terrain View' : '🗺️ Political View'}
      </button>
    </div>
  );
}
