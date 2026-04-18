import React, { useState, useRef, useEffect, useCallback } from 'react';
import type { TileData, LandBattleData, NavalBattleData, BattleData, ArchivedBattleTurn, BattleTile } from '../wasm';

// ── Hex rendering constants (mirror HexMap.tsx) ─────────────────
const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

const TERRAIN_COLORS: Record<string, string> = {
  Grassland: '#a8b860', Hills: '#9a8a68', Forest: '#3a7a3a',
  Mountain: '#7a7068', Desert: '#d8c888', Swamp: '#5a7a5a',
  Tundra: '#b8c8d0', Sea: '#4a88b8',
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

// ── Types ───────────────────────────────────────────────────────
interface Props {
  currentBattles: LandBattleData[];
  currentNavalBattles: NavalBattleData[];
  archiveData: ArchivedBattleTurn[];
  tiles: TileData[];
  year: number;
  quarter: number;
  onClose: () => void;
}

// ── Component ───────────────────────────────────────────────────
export default function BattleScreen({
  currentBattles, currentNavalBattles, archiveData, tiles,
  year, quarter, onClose,
}: Props) {
  const [mode, setMode] = useState<'current' | 'archive'>('current');
  const [selectedArchiveTurn, setSelectedArchiveTurn] = useState<number | null>(null);
  const [selectedBattleIdx, setSelectedBattleIdx] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Determine which battles to show based on mode
  const activeBattles: LandBattleData[] = mode === 'current'
    ? currentBattles
    : (archiveData.find(a => a.turn === selectedArchiveTurn)?.battles ?? []);
  const activeNavalBattles: NavalBattleData[] = mode === 'current'
    ? currentNavalBattles
    : (archiveData.find(a => a.turn === selectedArchiveTurn)?.naval_battles ?? []);

  const allBattles: BattleData[] = [
    ...activeBattles,
    ...activeNavalBattles,
  ];

  // Clamp selection
  const clampedIdx = allBattles.length > 0 ? Math.min(selectedBattleIdx, allBattles.length - 1) : 0;
  const selectedBattle: BattleData | null = allBattles[clampedIdx] ?? null;

  // Active turn label
  const activeTurnData = mode === 'archive' && selectedArchiveTurn
    ? archiveData.find(a => a.turn === selectedArchiveTurn)
    : null;
  const displayYear = mode === 'archive' && activeTurnData ? activeTurnData.year : year;
  const displayQuarter = mode === 'archive' && activeTurnData ? activeTurnData.quarter : quarter;

  // Archive turns sorted most recent first
  const archiveTurns = [...archiveData].sort((a, b) => b.turn - a.turn);

  // Auto-select first archive turn when switching to archive mode
  useEffect(() => {
    if (mode === 'archive' && selectedArchiveTurn === null && archiveTurns.length > 0) {
      setSelectedArchiveTurn(archiveTurns[0].turn);
    }
  }, [mode, selectedArchiveTurn, archiveTurns]);

  // Reset selection when battles change
  useEffect(() => {
    setSelectedBattleIdx(0);
  }, [mode, selectedArchiveTurn]);

  const handleSelectBattle = useCallback((idx: number) => {
    setSelectedBattleIdx(idx);
  }, []);

  // ── Mini-map rendering ──────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = '#0a0a1e';
    ctx.fillRect(0, 0, w, h);

    if (!selectedBattle || selectedBattle.type !== 'land') {
      // Naval or no battle — draw placeholder
      ctx.fillStyle = '#4a88b8';
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = '#e0d8c0';
      ctx.font = '16px Georgia';
      ctx.textAlign = 'center';
      if (selectedBattle?.type === 'naval') {
        ctx.fillText('Naval Engagement', w / 2, h / 2 - 10);
        ctx.font = '12px Georgia';
        ctx.fillText(`${selectedBattle.attacker} vs ${selectedBattle.defender}`, w / 2, h / 2 + 15);
      } else if (allBattles.length === 0) {
        ctx.fillStyle = '#666';
        ctx.fillText('No battles this turn', w / 2, h / 2);
      }
      return;
    }

    const battle = selectedBattle as LandBattleData;
    if (!battle.capital_tile) return;

    const centerQ = battle.capital_tile.q;
    const centerR = battle.capital_tile.r;
    const [centerPx, centerPy] = hexToPixel(centerQ, centerR);

    // Collect province tile keys for highlighting
    const provinceTileKeys = new Set(
      battle.province_tiles.map((t: BattleTile) => `${t.q},${t.r}`)
    );

    // Filter tiles within rendering radius (~6 hexes from center)
    const RADIUS = 6;
    const nearbyTiles = tiles.filter(t => {
      const dq = t.q - centerQ;
      const dr = t.r - centerR;
      const ds = -dq - dr;
      return Math.max(Math.abs(dq), Math.abs(dr), Math.abs(ds)) <= RADIUS;
    });

    // Calculate offset to center the battle province
    const offsetX = w / 2 - centerPx;
    const offsetY = h / 2 - centerPy;

    // Pass 1: Fill hexes
    for (const tile of nearbyTiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      const sx = px + offsetX;
      const sy = py + offsetY;

      const key = `${tile.q},${tile.r}`;
      const isBattleTile = provinceTileKeys.has(key);

      let color = TERRAIN_COLORS[tile.terrain] || '#666';
      if (tile.terrain === 'Sea') color = TERRAIN_COLORS.Sea;

      // Tint owned tiles with nation color
      if (tile.owner_color && NATION_COLORS[tile.owner_color]) {
        const nc = NATION_COLORS[tile.owner_color];
        // Blend: 70% terrain, 30% nation
        color = blendColors(color, nc, 0.3);
      }

      drawHexagon(ctx, sx, sy, HEX_SIZE);
      ctx.fillStyle = color;
      ctx.fill();

      // Highlight battle province tiles
      if (isBattleTile) {
        drawHexagon(ctx, sx, sy, HEX_SIZE);
        ctx.fillStyle = 'rgba(255, 80, 40, 0.35)';
        ctx.fill();
      }
    }

    // Pass 2: Borders
    ctx.strokeStyle = 'rgba(0,0,0,0.15)';
    ctx.lineWidth = 0.5;
    for (const tile of nearbyTiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      drawHexagon(ctx, px + offsetX, py + offsetY, HEX_SIZE);
      ctx.stroke();
    }

    // Pass 3: Province/country borders (thicker)
    for (const tile of nearbyTiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      const neighbors: [number, number][] = [
        [tile.q + 1, tile.r], [tile.q, tile.r + 1], [tile.q - 1, tile.r + 1],
        [tile.q - 1, tile.r], [tile.q, tile.r - 1], [tile.q + 1, tile.r - 1],
      ];
      for (const [nq, nr] of neighbors) {
        const neighbor = nearbyTiles.find(t => t.q === nq && t.r === nr);
        if (!neighbor) continue;
        if (tile.nation_id !== neighbor.nation_id) {
          const [npx, npy] = hexToPixel(nq, nr);
          const midX = (px + npx) / 2 + offsetX;
          const midY = (py + npy) / 2 + offsetY;
          ctx.strokeStyle = 'rgba(0,0,0,0.6)';
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.arc(midX, midY, 1, 0, Math.PI * 2);
          ctx.stroke();
        }
      }
    }

    // Pass 4: Battle province capital marker
    {
      const [cpx, cpy] = hexToPixel(centerQ, centerR);
      ctx.fillStyle = '#ff4040';
      ctx.font = 'bold 14px Georgia';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText('\u2694', cpx + offsetX, cpy + offsetY); // crossed swords
    }

    // Pass 5: Attack arrows from origin provinces
    if (battle.origin_tiles.length > 0) {
      ctx.strokeStyle = '#ff3333';
      ctx.lineWidth = 2.5;
      ctx.setLineDash([]);
      const [toPx, toPy] = hexToPixel(centerQ, centerR);
      const toSx = toPx + offsetX;
      const toSy = toPy + offsetY;

      for (const origin of battle.origin_tiles) {
        const [fromPx, fromPy] = hexToPixel(origin.q, origin.r);
        const fromSx = fromPx + offsetX;
        const fromSy = fromPy + offsetY;

        ctx.beginPath();
        ctx.moveTo(fromSx, fromSy);
        ctx.lineTo(toSx, toSy);
        ctx.stroke();

        // Arrowhead
        const angle = Math.atan2(toSy - fromSy, toSx - fromSx);
        const arrowLen = 8;
        ctx.beginPath();
        ctx.moveTo(toSx, toSy);
        ctx.lineTo(toSx - arrowLen * Math.cos(angle - 0.4), toSy - arrowLen * Math.sin(angle - 0.4));
        ctx.moveTo(toSx, toSy);
        ctx.lineTo(toSx - arrowLen * Math.cos(angle + 0.4), toSy - arrowLen * Math.sin(angle + 0.4));
        ctx.stroke();
      }
    }
  }, [selectedBattle, tiles, allBattles.length]);

  const hasBattles = allBattles.length > 0;

  return (
    <div style={styles.overlay}>
      <div style={styles.container}>
        {/* Header */}
        <div style={styles.header}>
          <h2 style={styles.title}>Battles</h2>
          <span style={styles.turnLabel}>{displayYear} Q{displayQuarter}</span>
          <div style={styles.modeTabs}>
            <button
              style={mode === 'current' ? styles.modeTabActive : styles.modeTab}
              onClick={() => { setMode('current'); }}
            >Current</button>
            <button
              style={mode === 'archive' ? styles.modeTabActive : styles.modeTab}
              onClick={() => { setMode('archive'); }}
            >Archive</button>
          </div>
          <button onClick={onClose} style={styles.closeBtn}>Esc</button>
        </div>

        {/* Body */}
        <div style={styles.body}>
          {/* Archive sidebar (only in archive mode) */}
          {mode === 'archive' && (
            <div style={styles.archiveSidebar}>
              <div style={styles.archiveTitle}>Past Turns</div>
              {archiveTurns.length === 0 && (
                <p style={styles.emptyText}>No battles in history yet.</p>
              )}
              {archiveTurns.map(a => (
                <button
                  key={a.turn}
                  style={a.turn === selectedArchiveTurn ? styles.archiveItemActive : styles.archiveItem}
                  onClick={() => setSelectedArchiveTurn(a.turn)}
                >
                  {a.year} Q{a.quarter}
                  <span style={styles.archiveBadge}>
                    {a.battles.length + a.naval_battles.length}
                  </span>
                </button>
              ))}
            </div>
          )}

          {/* Main content area */}
          <div style={styles.mainContent}>
            {!hasBattles ? (
              <div style={styles.emptyContainer}>
                <p style={styles.emptyText}>
                  {mode === 'current'
                    ? 'No battles occurred this turn.'
                    : 'Select a turn from the archive to view past battles.'}
                </p>
              </div>
            ) : (
              <div style={styles.battleLayout}>
                {/* Left column: mini-map + battle list */}
                <div style={styles.leftCol}>
                  {/* Mini-map */}
                  <div style={styles.miniMapContainer}>
                    <canvas
                      ref={canvasRef}
                      width={360}
                      height={260}
                      style={styles.miniMapCanvas}
                    />
                  </div>
                  {/* Battle list */}
                  <div style={styles.battleList}>
                    <div style={styles.battleListTitle}>Engagements</div>
                    {activeBattles.map((b, i) => (
                      <button
                        key={`land-${i}`}
                        style={clampedIdx === i ? styles.battleItemActive : styles.battleItem}
                        onClick={() => handleSelectBattle(i)}
                      >
                        <span style={styles.battleIcon}>{'\u2694'}</span>
                        <span>Battle of {b.province}</span>
                        <span style={{ color: b.attacker_won ? '#2ecc40' : '#e63946', marginLeft: 'auto', fontSize: 11 }}>
                          {b.attacker_won ? b.attacker : b.defender} won
                        </span>
                      </button>
                    ))}
                    {activeNavalBattles.map((nb, i) => {
                      const globalIdx = activeBattles.length + i;
                      return (
                        <button
                          key={`naval-${i}`}
                          style={clampedIdx === globalIdx ? styles.battleItemActive : styles.battleItem}
                          onClick={() => handleSelectBattle(globalIdx)}
                        >
                          <span style={styles.battleIcon}>{'\u2693'}</span>
                          <span>{nb.attacker} vs {nb.defender}</span>
                          <span style={{ color: nb.attacker_won ? '#2ecc40' : '#e63946', marginLeft: 'auto', fontSize: 11 }}>
                            {nb.attacker_won ? nb.attacker : nb.defender} won
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                {/* Right column: battle details */}
                <div style={styles.rightCol}>
                  {selectedBattle && selectedBattle.type === 'land' && (
                    <LandBattleDetails battle={selectedBattle} />
                  )}
                  {selectedBattle && selectedBattle.type === 'naval' && (
                    <NavalBattleDetails battle={selectedBattle} />
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Land battle details sub-component ───────────────────────────
function LandBattleDetails({ battle }: { battle: LandBattleData }) {
  const winnerName = battle.attacker_won ? battle.attacker : battle.defender;
  return (
    <div style={styles.detailsPanel}>
      {/* Outcome banner */}
      <div style={{
        ...styles.outcomeBanner,
        borderLeftColor: battle.attacker_won ? '#2ecc40' : '#e63946',
      }}>
        <div style={styles.outcomeTitle}>
          {winnerName} Victory
        </div>
        <div style={styles.outcomeSub}>
          at {battle.province}
          {battle.retreated && ' (attacker retreated)'}
        </div>
      </div>

      {/* Forces summary */}
      <div style={styles.detailSection}>
        <div style={styles.sectionTitle}>Forces</div>
        <div style={styles.forcesGrid}>
          <div style={styles.forceCol}>
            <div style={styles.forceHeader}>
              {battle.attacker}
              <span style={styles.roleTag}> (Attacker)</span>
            </div>
            <div style={styles.forceStats}>
              {battle.attacker_initial_count} units engaged
            </div>
            <div style={styles.forceStats}>
              {battle.attacker_survivors_count} survived
            </div>
            {battle.attacker_casualties.length > 0 && (
              <div style={styles.casualties}>
                <span style={styles.casualtyLabel}>Lost: </span>
                {formatCasualties(battle.attacker_casualties)}
              </div>
            )}
          </div>
          <div style={styles.forceCol}>
            <div style={styles.forceHeader}>
              {battle.defender}
              <span style={styles.roleTag}> (Defender)</span>
            </div>
            <div style={styles.forceStats}>
              {battle.defender_initial_count} units engaged
            </div>
            <div style={styles.forceStats}>
              {battle.defender_survivors_count} survived
            </div>
            {battle.defender_casualties.length > 0 && (
              <div style={styles.casualties}>
                <span style={styles.casualtyLabel}>Lost: </span>
                {formatCasualties(battle.defender_casualties)}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Terrain & Fort */}
      <div style={styles.detailSection}>
        <div style={styles.sectionTitle}>Battlefield</div>
        <div style={styles.fieldGrid}>
          {battle.terrain && (
            <div style={styles.fieldItem}>
              <span style={styles.fieldLabel}>Terrain:</span> {battle.terrain}
            </div>
          )}
          <div style={styles.fieldItem}>
            <span style={styles.fieldLabel}>Fort Level:</span> {battle.fort_level}
            {battle.siege_reduced_fort && <span style={styles.siegeNote}> (reduced by siege)</span>}
          </div>
        </div>
      </div>

      {/* Medal awards */}
      {battle.medal_awards.length > 0 && (
        <div style={styles.detailSection}>
          <div style={styles.sectionTitle}>Medals Awarded</div>
          {battle.medal_awards.map((m, i) => (
            <div key={i} style={styles.medalItem}>
              {'\u2605'} {m.unit_type} — {m.medals} medal{m.medals > 1 ? 's' : ''}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Naval battle details sub-component ──────────────────────────
function NavalBattleDetails({ battle }: { battle: NavalBattleData }) {
  const winnerName = battle.attacker_won ? battle.attacker : battle.defender;

  return (
    <div style={styles.detailsPanel}>
      <div style={{
        ...styles.outcomeBanner,
        borderLeftColor: battle.attacker_won ? '#2ecc40' : '#e63946',
      }}>
        <div style={styles.outcomeTitle}>
          {winnerName} Naval Victory
        </div>
        <div style={styles.outcomeSub}>
          {battle.attacker} vs {battle.defender}
        </div>
      </div>

      <div style={styles.detailSection}>
        <div style={styles.sectionTitle}>Fleets</div>
        <div style={styles.forcesGrid}>
          <div style={styles.forceCol}>
            <div style={styles.forceHeader}>{battle.attacker}</div>
            <div style={styles.forceStats}>{battle.attacker_survivors_count} ships survived</div>
            {battle.attacker_ships_lost.length > 0 && (
              <div style={styles.casualties}>
                <span style={styles.casualtyLabel}>Lost: </span>
                {formatCasualties(battle.attacker_ships_lost)}
              </div>
            )}
          </div>
          <div style={styles.forceCol}>
            <div style={styles.forceHeader}>{battle.defender}</div>
            <div style={styles.forceStats}>{battle.defender_survivors_count} ships survived</div>
            {battle.defender_ships_lost.length > 0 && (
              <div style={styles.casualties}>
                <span style={styles.casualtyLabel}>Lost: </span>
                {formatCasualties(battle.defender_ships_lost)}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Helpers ─────────────────────────────────────────────────────
function formatCasualties(types: string[]): string {
  const counts: Record<string, number> = {};
  for (const t of types) {
    counts[t] = (counts[t] || 0) + 1;
  }
  return Object.entries(counts)
    .map(([t, c]) => c > 1 ? `${c}x ${t}` : t)
    .join(', ');
}

function blendColors(base: string, overlay: string, amount: number): string {
  const parseHex = (h: string) => {
    const c = h.replace('#', '');
    return [parseInt(c.slice(0, 2), 16), parseInt(c.slice(2, 4), 16), parseInt(c.slice(4, 6), 16)];
  };
  const [br, bg, bb] = parseHex(base);
  const [or, og, ob] = parseHex(overlay);
  const r = Math.round(br * (1 - amount) + or * amount);
  const g = Math.round(bg * (1 - amount) + og * amount);
  const b = Math.round(bb * (1 - amount) + ob * amount);
  return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
}

// ── Styles ──────────────────────────────────────────────────────
const styles: Record<string, React.CSSProperties> = {
  overlay: {
    flex: 1, minHeight: 0,
    background: '#1a1a2e', color: '#e0d8c0',
    display: 'flex', flexDirection: 'column',
    fontFamily: "'Georgia', serif",
  },
  container: {
    display: 'flex', flexDirection: 'column', height: '100%',
  },
  header: {
    display: 'flex', alignItems: 'center', gap: 16,
    padding: '12px 24px', borderBottom: '2px solid #3a3520',
    background: '#0f0f23',
  },
  title: { color: '#daa520', margin: 0, fontSize: 22 },
  turnLabel: { color: '#aaa', fontSize: 14 },
  modeTabs: { display: 'flex', gap: 4, marginLeft: 'auto' },
  modeTab: {
    padding: '4px 12px', background: '#1a1a2e', color: '#888',
    border: '1px solid #3a3520', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 12,
  },
  modeTabActive: {
    padding: '4px 12px', background: '#3a3520', color: '#daa520',
    border: '1px solid #5a5030', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 12,
  },
  closeBtn: {
    padding: '4px 12px', background: '#3a3520', color: '#e0d8c0',
    border: '1px solid #5a5030', cursor: 'pointer', fontFamily: "'Georgia', serif",
  },
  body: {
    flex: 1, minHeight: 0, display: 'flex', overflow: 'hidden',
  },
  archiveSidebar: {
    width: 140, borderRight: '1px solid #3a3520', background: '#12122a',
    overflowY: 'auto', padding: '8px 0',
  },
  archiveTitle: {
    padding: '4px 12px', fontSize: 11, color: '#888', textTransform: 'uppercase' as const,
    letterSpacing: 1,
  },
  archiveItem: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    width: '100%', padding: '6px 12px', background: 'none', color: '#e0d8c0',
    border: 'none', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 13,
    textAlign: 'left' as const,
  },
  archiveItemActive: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    width: '100%', padding: '6px 12px', background: '#2a2a4e', color: '#daa520',
    border: 'none', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 13,
    textAlign: 'left' as const, borderLeft: '3px solid #daa520',
  },
  archiveBadge: {
    background: '#3a3520', color: '#daa520', borderRadius: 8,
    padding: '1px 6px', fontSize: 10, fontWeight: 'bold',
  },
  mainContent: {
    flex: 1, minHeight: 0, overflow: 'auto',
  },
  emptyContainer: {
    display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100%',
  },
  emptyText: {
    color: '#666', fontStyle: 'italic', fontSize: 14,
  },
  battleLayout: {
    display: 'flex', height: '100%',
  },
  leftCol: {
    width: 380, display: 'flex', flexDirection: 'column',
    borderRight: '1px solid #3a3520',
  },
  miniMapContainer: {
    padding: 12, borderBottom: '1px solid #3a3520',
    display: 'flex', justifyContent: 'center',
  },
  miniMapCanvas: {
    borderRadius: 4, border: '1px solid #3a3520',
  },
  battleList: {
    flex: 1, overflowY: 'auto', padding: '8px 0',
  },
  battleListTitle: {
    padding: '4px 12px', fontSize: 11, color: '#888', textTransform: 'uppercase' as const,
    letterSpacing: 1,
  },
  battleItem: {
    display: 'flex', alignItems: 'center', gap: 8,
    width: '100%', padding: '8px 12px', background: 'none', color: '#e0d8c0',
    border: 'none', borderBottom: '1px solid #2a2a3e', cursor: 'pointer',
    fontFamily: "'Georgia', serif", fontSize: 13, textAlign: 'left' as const,
  },
  battleItemActive: {
    display: 'flex', alignItems: 'center', gap: 8,
    width: '100%', padding: '8px 12px', background: '#2a2a4e', color: '#daa520',
    border: 'none', borderBottom: '1px solid #2a2a3e', cursor: 'pointer',
    fontFamily: "'Georgia', serif", fontSize: 13, textAlign: 'left' as const,
    borderLeft: '3px solid #daa520',
  },
  battleIcon: { fontSize: 16 },
  rightCol: {
    flex: 1, overflowY: 'auto', padding: 16,
  },
  detailsPanel: {
    display: 'flex', flexDirection: 'column', gap: 16,
  },
  outcomeBanner: {
    background: '#12122a', padding: '16px 20px', borderRadius: 4,
    borderLeft: '4px solid #daa520',
  },
  outcomeTitle: {
    fontSize: 20, fontWeight: 'bold', color: '#daa520',
  },
  outcomeSub: {
    fontSize: 13, color: '#aaa', marginTop: 4,
  },
  detailSection: {
    background: '#12122a', padding: '12px 16px', borderRadius: 4,
  },
  sectionTitle: {
    fontSize: 12, color: '#888', textTransform: 'uppercase' as const,
    letterSpacing: 1, marginBottom: 8, borderBottom: '1px solid #2a2a3e', paddingBottom: 4,
  },
  forcesGrid: {
    display: 'flex', gap: 24,
  },
  forceCol: {
    flex: 1,
  },
  forceHeader: {
    fontSize: 14, fontWeight: 'bold', color: '#e0d8c0', marginBottom: 4,
  },
  roleTag: {
    fontSize: 11, color: '#888', fontWeight: 'normal',
  },
  forceStats: {
    fontSize: 13, color: '#bbb', marginBottom: 2,
  },
  casualties: {
    fontSize: 12, color: '#e63946', marginTop: 4,
  },
  casualtyLabel: {
    color: '#aaa',
  },
  fieldGrid: {
    display: 'flex', gap: 24, flexWrap: 'wrap' as const,
  },
  fieldItem: {
    fontSize: 13, color: '#bbb',
  },
  fieldLabel: {
    color: '#aaa',
  },
  siegeNote: {
    color: '#daa520', fontSize: 11,
  },
  medalItem: {
    fontSize: 13, color: '#daa520', marginBottom: 2,
  },
};
