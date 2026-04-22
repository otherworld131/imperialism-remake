import { useEffect, useRef } from 'react';
import type { PoliticalSnapshot, PoliticalSnapshotTile } from '../wasm';
import { computeNationLabels } from '../lib/nationLabels';

const HEX_SIZE = 14;
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

function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
}

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
    [q + 1, r], [q, r + 1], [q - 1, r + 1],
    [q - 1, r], [q, r - 1], [q + 1, r - 1],
  ];
}

function politicalFill(nationHex: string): string {
  const c = parseInt(nationHex.slice(1), 16);
  const r = (c >> 16) & 0xff, g = (c >> 8) & 0xff, b = c & 0xff;
  return `rgb(${Math.min(255, r + Math.round((255 - r) * 0.45))},${Math.min(255, g + Math.round((255 - g) * 0.45))},${Math.min(255, b + Math.round((255 - b) * 0.45))})`;
}

function incorporatedFill(nationHex: string): string {
  const c = parseInt(nationHex.slice(1), 16);
  const r = (c >> 16) & 0xff, g = (c >> 8) & 0xff, b = c & 0xff;
  return `rgb(${Math.min(255, r + Math.round((255 - r) * 0.65))},${Math.min(255, g + Math.round((255 - g) * 0.65))},${Math.min(255, b + Math.round((255 - b) * 0.65))})`;
}

function pickFill(tile: PoliticalSnapshotTile): string {
  if (tile.terrain === 'Sea') return TERRAIN_COLORS.Sea;
  if (!tile.owner_color) return TERRAIN_COLORS[tile.terrain] || '#666';
  const nc = NATION_COLORS[tile.owner_color];
  if (!nc) return TERRAIN_COLORS[tile.terrain] || '#666';
  return tile.is_incorporated_minor ? incorporatedFill(nc) : politicalFill(nc);
}

interface Props {
  snapshot: PoliticalSnapshot;
  onClose: () => void;
}

export default function PoliticalMapModal({ snapshot, onClose }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const cssW = canvas.clientWidth;
    const cssH = canvas.clientHeight;
    canvas.width = Math.round(cssW * dpr);
    canvas.height = Math.round(cssH * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);

    if (snapshot.tiles.length === 0) return;

    // Compute bounds in pixel space
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const t of snapshot.tiles) {
      const [px, py] = hexToPixel(t.q, t.r);
      if (px < minX) minX = px;
      if (px > maxX) maxX = px;
      if (py < minY) minY = py;
      if (py > maxY) maxY = py;
    }
    const pad = HEX_SIZE * 2;
    const boundsW = (maxX - minX) + pad * 2;
    const boundsH = (maxY - minY) + pad * 2;
    const scale = Math.min(cssW / boundsW, cssH / boundsH);
    const offsetX = (cssW - (maxX + minX) * scale) / 2;
    const offsetY = (cssH - (maxY + minY) * scale) / 2;

    ctx.save();
    ctx.translate(offsetX, offsetY);
    ctx.scale(scale, scale);

    // Tile lookup for border detection
    const tileMap = new Map<string, PoliticalSnapshotTile>();
    for (const t of snapshot.tiles) tileMap.set(`${t.q},${t.r}`, t);

    const verts = hexVertices(HEX_SIZE);

    // Pass 1: fills
    for (const tile of snapshot.tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      ctx.beginPath();
      for (let i = 0; i < 6; i++) {
        const [vx, vy] = verts[i];
        if (i === 0) ctx.moveTo(px + vx, py + vy);
        else ctx.lineTo(px + vx, py + vy);
      }
      ctx.closePath();
      ctx.fillStyle = pickFill(tile);
      ctx.fill();
    }

    // Pass 2: borders — country borders thick, province medium, normal thin
    const normalSegs: number[] = [];
    const provinceSegs: number[] = [];
    const countrySegs: number[] = [];

    for (const tile of snapshot.tiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      const neighbors = hexNeighbors(tile.q, tile.r);
      // Group key matches HexMap semantics: incorporated minors keep a
      // separate country-level border from their overlord via visual_group.
      const tileVG = tile.visual_group || tile.owner;
      for (let i = 0; i < 6; i++) {
        const [nq, nr] = neighbors[i];
        const neighbor = tileMap.get(`${nq},${nr}`);
        const neighborVG = neighbor ? (neighbor.visual_group || neighbor.owner) : '';
        let borderType: 0 | 1 | 2;
        if (tile.terrain === 'Sea') {
          borderType = 0;
        } else if (!neighbor || neighbor.terrain === 'Sea') {
          borderType = tile.owner ? 2 : 0;
        } else if (tileVG !== neighborVG) {
          borderType = 2;
        } else if (tile.owner && tile.province !== neighbor.province) {
          borderType = 1;
        } else {
          borderType = 0;
        }
        const [vx1, vy1] = verts[i];
        const [vx2, vy2] = verts[(i + 1) % 6];
        const x1 = px + vx1, y1 = py + vy1, x2 = px + vx2, y2 = py + vy2;
        if (borderType === 0) normalSegs.push(x1, y1, x2, y2);
        else if (borderType === 1) provinceSegs.push(x1, y1, x2, y2);
        else countrySegs.push(x1, y1, x2, y2);
      }
    }

    const drawSegs = (segs: number[], strokeStyle: string, lineWidth: number) => {
      ctx.strokeStyle = strokeStyle;
      ctx.lineWidth = lineWidth;
      ctx.beginPath();
      for (let i = 0; i < segs.length; i += 4) {
        ctx.moveTo(segs[i], segs[i + 1]);
        ctx.lineTo(segs[i + 2], segs[i + 3]);
      }
      ctx.stroke();
    };

    drawSegs(normalSegs, 'rgba(0,0,0,0.12)', 1);
    drawSegs(provinceSegs, 'rgba(40,30,20,0.55)', 1.5);
    drawSegs(countrySegs, 'rgba(20,10,0,0.9)', 2.5);

    // Capital markers
    ctx.fillStyle = '#1a1a1a';
    ctx.strokeStyle = '#fff';
    ctx.lineWidth = 1;
    for (const tile of snapshot.tiles) {
      if (!tile.is_country_capital) continue;
      const [px, py] = hexToPixel(tile.q, tile.r);
      ctx.beginPath();
      ctx.arc(px, py, HEX_SIZE * 0.3, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }

    // Nation name labels — same semantics as live political map
    const nationLabels = computeNationLabels(snapshot.tiles, 3, HEX_SIZE);
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const label of nationLabels) {
      const fontSize = Math.max(10, Math.min(22, Math.sqrt(label.size) * 2.4));
      ctx.font = `bold ${fontSize}px Georgia, serif`;
      ctx.lineWidth = 3;
      ctx.strokeStyle = 'rgba(0,0,0,0.55)';
      ctx.strokeText(label.name.toUpperCase(), label.cx, label.cy);
      ctx.fillStyle = 'rgba(255,255,255,0.9)';
      ctx.fillText(label.name.toUpperCase(), label.cx, label.cy);
    }

    ctx.restore();
  }, [snapshot]);

  // Build legend: unique nations with color
  const nationLegend: { name: string; colorHex: string }[] = [];
  const seen = new Set<string>();
  for (const t of snapshot.tiles) {
    if (!t.owner || seen.has(t.owner)) continue;
    seen.add(t.owner);
    const nc = NATION_COLORS[t.owner_color] || '#888';
    const color = t.is_incorporated_minor ? incorporatedFill(nc) : politicalFill(nc);
    nationLegend.push({ name: t.owner, colorHex: color });
  }

  return (
    <div style={styles.backdrop} onClick={onClose}>
      <div style={styles.modal} onClick={e => e.stopPropagation()}>
        <div style={styles.header}>
          <span style={styles.title}>
            Political Map — {snapshot.year} Q{snapshot.quarter} (Turn {snapshot.turn})
          </span>
          <button style={styles.closeBtn} onClick={onClose}>Close</button>
        </div>
        <div style={styles.body}>
          <canvas ref={canvasRef} style={styles.canvas} />
          <div style={styles.legend}>
            <div style={styles.legendTitle}>Nations</div>
            {nationLegend.map(n => (
              <div key={n.name} style={styles.legendRow}>
                <span style={{ ...styles.swatch, background: n.colorHex }} />
                <span>{n.name}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  backdrop: {
    position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.75)',
    display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 2000,
  },
  modal: {
    width: '92vw', height: '88vh', maxWidth: 1400,
    background: '#1a1a2e', color: '#e0d8c0',
    border: '2px solid #5a5030', borderRadius: 4,
    display: 'flex', flexDirection: 'column',
    fontFamily: 'Georgia, serif',
  },
  header: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '10px 16px', borderBottom: '1px solid #3a3520', background: '#0f0f23',
  },
  title: { fontSize: 16, color: '#daa520', fontWeight: 'bold' },
  closeBtn: {
    padding: '6px 14px', background: '#5a5030', color: '#e0d8c0',
    border: '1px solid #7a6540', cursor: 'pointer', fontFamily: 'Georgia, serif',
  },
  body: { display: 'flex', flex: 1, minHeight: 0 },
  canvas: { flex: 1, background: '#0a1a2a', display: 'block' },
  legend: {
    width: 180, overflowY: 'auto' as const, padding: '10px 12px',
    borderLeft: '1px solid #3a3520', background: '#0f0f23', fontSize: 13,
  },
  legendTitle: {
    color: '#daa520', fontWeight: 'bold', marginBottom: 8,
    textTransform: 'uppercase' as const, letterSpacing: 1,
  },
  legendRow: {
    display: 'flex', alignItems: 'center', gap: 8, padding: '3px 0',
  },
  swatch: {
    width: 14, height: 14, border: '1px solid #5a5030', display: 'inline-block',
  },
};
