import { useRef, useEffect, useState, useCallback } from 'react';
import type { TileData } from '../wasm';

// Hex geometry constants
const HEX_SIZE = 18;

// Color maps
const TERRAIN_COLORS: Record<string, string> = {
  Farm: '#99cc33', HardwoodForest: '#1a801a', ScrubForest: '#4d8c40',
  FertileHills: '#8cb35a', BarrenHills: '#8c734d', Mountain: '#807366',
  Sea: '#264da6', DryPlains: '#ccbf80', Plantation: '#66b34d',
  OpenRange: '#a6bf66', HorseRanch: '#b3a659', Orchard: '#80bf40',
  Swamp: '#4d664d', Desert: '#d9cc8c', Tundra: '#bfccd9',
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

function hexToPixel(q: number, r: number): [number, number] {
  const x = HEX_SIZE * (Math.sqrt(3) * q + Math.sqrt(3) / 2 * r);
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
  const [scale, setScale] = useState(1);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.save();
    ctx.translate(offset.x, offset.y);
    ctx.scale(scale, scale);

    // Draw tiles
    for (const tile of tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);

      // Determine color
      let color = TERRAIN_COLORS[tile.terrain] || '#666';
      if (tile.terrain !== 'Sea' && tile.owner_color) {
        color = NATION_COLORS[tile.owner_color] || color;
      }

      drawHexagon(ctx, px, py, HEX_SIZE - 1);
      ctx.fillStyle = color;
      ctx.fill();
      ctx.strokeStyle = 'rgba(0,0,0,0.2)';
      ctx.lineWidth = 0.5;
      ctx.stroke();

      // Terrain label
      if (tile.terrain !== 'Sea') {
        const label = tile.terrain[0];
        ctx.fillStyle = 'rgba(0,0,0,0.6)';
        ctx.font = '9px monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, px, py);
      }

      // Capital star
      if (tile.is_capital && tile.terrain !== 'Sea') {
        ctx.fillStyle = 'white';
        ctx.font = '12px serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('\u2605', px, py - 1);
      }
    }

    ctx.restore();
  }, [tiles, offset, scale]);

  useEffect(() => { render(); }, [render]);

  // Pan
  const handleMouseDown = (e: React.MouseEvent) => {
    setDragging(true);
    setDragStart({ x: e.clientX - offset.x, y: e.clientY - offset.y });
  };
  const handleMouseMove = (e: React.MouseEvent) => {
    if (dragging) {
      setOffset({ x: e.clientX - dragStart.x, y: e.clientY - dragStart.y });
    }
    // Hover detection
    if (onTileHover && canvasRef.current) {
      const rect = canvasRef.current.getBoundingClientRect();
      const mx = (e.clientX - rect.left - offset.x) / scale;
      const my = (e.clientY - rect.top - offset.y) / scale;
      // Find closest tile
      let closest: TileData | null = null;
      let minDist = Infinity;
      for (const tile of tiles) {
        const [px, py] = hexToPixel(tile.q, tile.r);
        const d = Math.hypot(mx - px, my - py);
        if (d < HEX_SIZE && d < minDist) { minDist = d; closest = tile; }
      }
      onTileHover(closest);
    }
  };
  const handleMouseUp = () => setDragging(false);

  // Zoom
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const newScale = Math.max(0.3, Math.min(4, scale - e.deltaY * 0.001));
    setScale(newScale);
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
        const d = Math.hypot(mx - px, my - py);
        if (d < HEX_SIZE && d < minDist) { minDist = d; closest = tile; }
      }
      if (closest) onTileClick(closest);
    }
  };

  return (
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
  );
}
