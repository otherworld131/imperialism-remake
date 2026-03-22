import { useRef, useEffect, useState, useCallback } from 'react';
import type { TileData } from '../wasm';

// Hex geometry constants
const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

// Terrain colors — each terrain type has a distinct color
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

// Nation colors used for tinting owned tiles
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

// Two zoom presets
const ZOOM_CLOSE = 2.0;
const ZOOM_FAR = 0.7;

function hexToPixel(q: number, r: number): [number, number] {
  const x = HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r);
  const y = HEX_SIZE * (3 / 2 * r);
  return [x, y];
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

// Get the 6 hex neighbors for pointy-top hexagons (axial coordinates)
function hexNeighbors(q: number, r: number): [number, number][] {
  return [
    [q + 1, r], [q + 1, r - 1], [q, r - 1],
    [q - 1, r], [q - 1, r + 1], [q, r + 1],
  ];
}

// Get the two vertices of hex edge `i` (0-5), for pointy-top hexagons
function hexEdgeVertices(
  cx: number, cy: number, size: number, edgeIndex: number,
): [[number, number], [number, number]] {
  const a1 = (Math.PI / 180) * (60 * edgeIndex - 30);
  const a2 = (Math.PI / 180) * (60 * ((edgeIndex + 1) % 6) - 30);
  return [
    [cx + size * Math.cos(a1), cy + size * Math.sin(a1)],
    [cx + size * Math.cos(a2), cy + size * Math.sin(a2)],
  ];
}

// Blend terrain color with a subtle nation tint
function tintColor(terrainHex: string, nationHex: string, amount: number): string {
  const tc = parseInt(terrainHex.slice(1), 16);
  const nc = parseInt(nationHex.slice(1), 16);
  const tr = (tc >> 16) & 0xff, tg = (tc >> 8) & 0xff, tb = tc & 0xff;
  const nr = (nc >> 16) & 0xff, ng = (nc >> 8) & 0xff, nb = nc & 0xff;
  const r = Math.round(tr * (1 - amount) + nr * amount);
  const g = Math.round(tg * (1 - amount) + ng * amount);
  const b = Math.round(tb * (1 - amount) + nb * amount);
  return `rgb(${r},${g},${b})`;
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

  // Compute the pixel width of the full map for wrapping
  const mapWidth = tiles.length > 0 ? (tiles[0].map_width || 60) : 60;
  const mapPixelWidth = HEX_SIZE * SQRT3 * mapWidth;

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Build lookup map
    const tileMap = new Map<string, TileData>();
    for (const tile of tiles) {
      tileMap.set(`${tile.q},${tile.r}`, tile);
    }

    // We draw the map 3 times side-by-side for horizontal wrapping
    const wraps = [-1, 0, 1];

    for (const wrapOffset of wraps) {
      ctx.save();
      ctx.translate(offset.x + wrapOffset * mapPixelWidth * scale, offset.y);
      ctx.scale(scale, scale);

      // Pass 1: Fill hexagons with terrain colors
      for (const tile of tiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);

        let color = TERRAIN_COLORS[tile.terrain] || '#666';

        // Subtle nation tint (15%) for owned non-sea tiles
        if (tile.terrain !== 'Sea' && tile.owner_color) {
          const nationColor = NATION_COLORS[tile.owner_color];
          if (nationColor) {
            color = tintColor(color, nationColor, 0.15);
          }
        }

        drawHexagon(ctx, px, py, HEX_SIZE - 0.5);
        ctx.fillStyle = color;
        ctx.fill();

        // Very thin hex outline
        ctx.strokeStyle = 'rgba(0,0,0,0.06)';
        ctx.lineWidth = 0.3;
        ctx.stroke();
      }

      // Pass 2: Draw borders
      for (const tile of tiles) {
        if (tile.terrain === 'Sea') continue;
        const [px, py] = hexToPixel(tile.q, tile.r);
        const neighbors = hexNeighbors(tile.q, tile.r);

        for (let i = 0; i < 6; i++) {
          const [nq, nr] = neighbors[i];
          const neighbor = tileMap.get(`${nq},${nr}`);

          let borderType: 'none' | 'country' | 'province' = 'none';

          if (!neighbor) {
            if (tile.owner) borderType = 'country';
          } else if (neighbor.terrain === 'Sea') {
            if (tile.owner) borderType = 'country';
          } else if (tile.owner !== neighbor.owner) {
            borderType = 'country';
          } else if (tile.owner && tile.province !== neighbor.province) {
            borderType = 'province';
          }

          if (borderType !== 'none') {
            const [[x1, y1], [x2, y2]] = hexEdgeVertices(px, py, HEX_SIZE - 0.5, i);
            ctx.lineCap = 'round';
            ctx.beginPath();
            ctx.moveTo(x1, y1);
            ctx.lineTo(x2, y2);

            if (borderType === 'country') {
              ctx.strokeStyle = 'rgba(0,0,0,0.8)';
              ctx.lineWidth = 2.5;
            } else {
              ctx.strokeStyle = 'rgba(0,0,0,0.3)';
              ctx.lineWidth = 1.0;
            }
            ctx.stroke();
          }
        }
      }

      // Pass 3: Capital markers (drawn last so they're on top)
      for (const tile of tiles) {
        if (!tile.is_capital || tile.terrain === 'Sea') continue;
        const [px, py] = hexToPixel(tile.q, tile.r);

        if (tile.is_country_capital) {
          // Country capital: large gold star with dark outline
          ctx.font = `bold ${Math.max(14, HEX_SIZE * 0.85)}px serif`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          // Dark outline
          ctx.fillStyle = 'rgba(0,0,0,0.7)';
          ctx.fillText('\u2605', px + 0.5, py + 0.5);
          // Gold fill
          ctx.fillStyle = '#ffd700';
          ctx.fillText('\u2605', px, py);
        } else {
          // Province capital: smaller white star
          ctx.font = `${Math.max(9, HEX_SIZE * 0.55)}px serif`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          ctx.fillStyle = 'rgba(255,255,255,0.85)';
          ctx.fillText('\u2605', px, py);
        }
      }

      ctx.restore();
    }
  }, [tiles, offset, scale, mapPixelWidth]);

  useEffect(() => { render(); }, [render]);

  // Pan (no vertical clamping — free scroll north/south)
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
        // Check against all wrap positions
        for (const w of [-1, 0, 1]) {
          const wpx = px + w * mapPixelWidth;
          const d = Math.hypot(mx - wpx, my - py);
          if (d < HEX_SIZE && d < minDist) { minDist = d; closest = tile; }
        }
      }
      onTileHover(closest);
    }
  };
  const handleMouseUp = () => setDragging(false);

  // Zoom with mouse wheel
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const newScale = Math.max(0.3, Math.min(4, scale - e.deltaY * 0.001));
    setScale(newScale);
  };

  // Toggle between two zoom levels
  const toggleZoom = () => {
    setScale(prev => prev < (ZOOM_CLOSE + ZOOM_FAR) / 2 ? ZOOM_CLOSE : ZOOM_FAR);
  };

  // Click
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
          const wpx = px + w * mapPixelWidth;
          const d = Math.hypot(mx - wpx, my - py);
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
      {/* Zoom toggle button */}
      <button
        onClick={toggleZoom}
        style={{
          position: 'absolute', bottom: 12, right: 12,
          padding: '6px 14px', background: '#3a3520', color: '#e0d8c0',
          border: '1px solid #5a5030', borderRadius: 4, cursor: 'pointer',
          fontSize: 13, fontFamily: 'Georgia, serif',
        }}
      >
        {scale < (ZOOM_CLOSE + ZOOM_FAR) / 2 ? '🔍 Zoom In' : '🗺️ Overview'}
      </button>
    </div>
  );
}
