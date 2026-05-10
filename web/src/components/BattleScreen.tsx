import React, { useState, useRef, useEffect, useCallback } from 'react';
import type { TileData, LandBattleData, NavalBattleData, BattleData, ArchivedBattleTurn, BattleTile, BattleUnit, BattleUnitLog, BattleRoundLog, RetreatDebug } from '../wasm';
import { UnitRow } from './UnitRow';
import { computeNationLabels } from '../lib/nationLabels';
import Flag from './Flag';

interface NationLite { id: number; flag_svg?: string; }

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

function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
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
  nations?: NationLite[];
  onClose: () => void;
  showRetreatDebug?: boolean;
  showFirepower?: boolean;
}

// ── Component ───────────────────────────────────────────────────
export default function BattleScreen({
  currentBattles, currentNavalBattles, archiveData, tiles,
  year, quarter, nations = [], onClose,
  showRetreatDebug,
  showFirepower = true,
}: Props) {
  const flagById: Record<number, string> = {};
  for (const n of nations) { if (n.flag_svg) flagById[n.id] = n.flag_svg; }
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

    // Filter tiles within rendering radius — wider than 6 so country names fit
    const RADIUS = 15;
    const nearbyTiles = tiles.filter(t => {
      const dq = t.q - centerQ;
      const dr = t.r - centerR;
      const ds = -dq - dr;
      return Math.max(Math.abs(dq), Math.abs(dr), Math.abs(ds)) <= RADIUS;
    });

    // Calculate offset to center the battle province
    const offsetX = w / 2 - centerPx;
    const offsetY = h / 2 - centerPy;

    // Pass 1: Fill hexes — matches HexMap political mode
    for (const tile of nearbyTiles) {
      const [px, py] = hexToPixel(tile.q, tile.r);
      const sx = px + offsetX;
      const sy = py + offsetY;

      const key = `${tile.q},${tile.r}`;
      const isBattleTile = provinceTileKeys.has(key);

      let color: string;
      if (tile.terrain === 'Sea') {
        color = TERRAIN_COLORS.Sea;
      } else if (tile.owner_color && NATION_COLORS[tile.owner_color]) {
        const nc = NATION_COLORS[tile.owner_color];
        color = tile.is_incorporated_minor ? incorporatedFill(nc) : politicalFill(nc);
      } else {
        color = TERRAIN_COLORS[tile.terrain] || '#666';
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

    // Pass 3b: Country name labels (political map)
    {
      const labels = computeNationLabels(nearbyTiles, 5);
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      for (const label of labels) {
        const fontSize = Math.max(11, Math.min(22, Math.sqrt(label.size) * 3));
        ctx.font = `bold ${fontSize}px Georgia, serif`;
        ctx.lineWidth = 3;
        if (label.is_anarchic) {
          ctx.strokeStyle = 'rgba(255,255,255,0.55)';
          ctx.strokeText(label.name.toUpperCase(), label.cx + offsetX, label.cy + offsetY);
          ctx.fillStyle = 'rgba(0,0,0,0.95)';
        } else {
          ctx.strokeStyle = 'rgba(0,0,0,0.5)';
          ctx.strokeText(label.name.toUpperCase(), label.cx + offsetX, label.cy + offsetY);
          ctx.fillStyle = 'rgba(255,255,255,0.9)';
        }
        ctx.fillText(label.name.toUpperCase(), label.cx + offsetX, label.cy + offsetY);
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
                    {activeBattles.map((b, i) => {
                      const winnerId = b.attacker_won ? b.attacker_id : b.defender_id;
                      const winnerFlag = flagById[winnerId];
                      return (
                        <button
                          key={`land-${i}`}
                          style={clampedIdx === i ? styles.battleItemActive : styles.battleItem}
                          onClick={() => handleSelectBattle(i)}
                        >
                          <span style={styles.battleIcon}>{'\u2694'}</span>
                          <span>Battle of {b.province}</span>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, color: b.attacker_won ? '#2ecc40' : '#e63946', marginLeft: 'auto', fontSize: 'var(--ui-font-size, 14px)' }}>
                            {winnerFlag && <Flag svg={winnerFlag} width={16} height={11} />}
                            {b.attacker_won ? b.attacker : b.defender} won
                          </span>
                        </button>
                      );
                    })}
                    {activeNavalBattles.map((nb, i) => {
                      const globalIdx = activeBattles.length + i;
                      const winnerId = nb.attacker_won ? nb.attacker_id : nb.defender_id;
                      const winnerFlag = flagById[winnerId];
                      return (
                        <button
                          key={`naval-${i}`}
                          style={clampedIdx === globalIdx ? styles.battleItemActive : styles.battleItem}
                          onClick={() => handleSelectBattle(globalIdx)}
                        >
                          <span style={styles.battleIcon}>{'\u2693'}</span>
                          <span>{nb.attacker} vs {nb.defender}</span>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, color: nb.attacker_won ? '#2ecc40' : '#e63946', marginLeft: 'auto', fontSize: 'var(--ui-font-size, 14px)' }}>
                            {winnerFlag && <Flag svg={winnerFlag} width={16} height={11} />}
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
                    <LandBattleDetails
                      battle={selectedBattle}
                      flagById={flagById}
                      showRetreatDebug={showRetreatDebug}
                      showFirepower={showFirepower}
                    />
                  )}
                  {selectedBattle && selectedBattle.type === 'naval' && (
                    <NavalBattleDetails battle={selectedBattle} flagById={flagById} />
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
function LandBattleDetails({
  battle,
  flagById,
  showRetreatDebug,
  showFirepower,
}: {
  battle: LandBattleData;
  flagById: Record<number, string>;
  showRetreatDebug?: boolean;
  showFirepower?: boolean;
}) {
  const winnerName = battle.attacker_won ? battle.attacker : battle.defender;
  const winnerId = battle.attacker_won ? battle.attacker_id : battle.defender_id;
  const winnerFlag = flagById[winnerId];
  return (
    <div style={styles.detailsPanel}>
      {/* Outcome banner */}
      <div style={{
        ...styles.outcomeBanner,
        borderLeftColor: battle.attacker_won ? '#2ecc40' : '#e63946',
      }}>
        <div style={{ ...styles.outcomeTitle, display: 'flex', alignItems: 'center', gap: 8 }}>
          {winnerFlag && <Flag svg={winnerFlag} width={24} height={16} />}
          {winnerName} Victory
        </div>
        <div style={styles.outcomeSub}>
          at {battle.province}
          {battle.retreated && ' (attacker retreated)'}
        </div>
        {(battle.is_naval_landing || battle.origin_province_names.length > 0) && (
          <div style={{ marginTop: 6, fontSize: 'var(--ui-font-size, 14px)', color: '#d8d0b8' }}>
            {battle.is_naval_landing && (
              <span style={{
                display: 'inline-block', marginRight: 6, padding: '1px 6px',
                background: 'rgba(218,165,32,0.15)', border: '1px solid #daa520',
                color: '#daa520', borderRadius: 3, fontSize: 11,
              }}>
                {'\u{1F6A2}'} Naval Landing
              </span>
            )}
            {battle.origin_province_names.length > 0 && (
              <span>
                <span style={{ color: '#9a9a9a' }}>Origin:</span>{' '}
                {battle.origin_province_names.join(', ')}
              </span>
            )}
          </div>
        )}
      </div>

      {/* Terrain & Fort — shown above Forces so the setting reads first */}
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

      {showRetreatDebug && battle.retreat_debug && (
        <div style={styles.detailSection}>
          <div style={styles.sectionTitle}>Retreat math (debug)</div>
          <RetreatDebugBlock debug={battle.retreat_debug} battle={battle} />
        </div>
      )}

      {/* Forces — per-unit cards (main-map sidebar style) */}
      <div style={styles.detailSection}>
        <div style={styles.sectionTitle}>Forces</div>
        <div style={styles.forcesGrid}>
          <ForceColumn
            side={battle.attacker}
            role="Attacker"
            flag={flagById[battle.attacker_id]}
            initial={battle.attacker_initial_count}
            survivedCount={battle.attacker_survivors_count}
            survivors={battle.attacker_survivors}
            casualties={battle.attacker_casualties}
            unitLogs={battle.attacker_unit_logs}
            showFirepower={showFirepower}
          />
          <ForceColumn
            side={battle.defender}
            role="Defender"
            flag={flagById[battle.defender_id]}
            initial={battle.defender_initial_count}
            survivedCount={battle.defender_survivors_count}
            survivors={battle.defender_survivors}
            casualties={battle.defender_casualties}
            unitLogs={battle.defender_unit_logs}
            showFirepower={showFirepower}
          />
        </div>
      </div>

      {showFirepower && (
        <div style={styles.detailSection}>
          <div style={styles.sectionTitle}>How combat is calculated</div>
          <CombatExplanation battle={battle} />
        </div>
      )}

      {showFirepower && battle.round_logs && battle.round_logs.length > 0 && (
        <div style={styles.detailSection}>
          <div style={styles.sectionTitle}>How the battle played out</div>
          <RoundPlayout battle={battle} />
        </div>
      )}

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

// ── Retreat-math debug block ────────────────────────────────────
function RetreatDebugBlock({ debug, battle }: { debug: RetreatDebug; battle: LandBattleData }) {
  const fmt = (n: number) => (Number.isFinite(n) ? n.toFixed(2) : String(n));
  const labelStyle: React.CSSProperties = { color: '#aab', fontSize: 11 };
  const numStyle: React.CSSProperties = { color: '#fff', fontFamily: 'monospace' };

  let summary: React.ReactNode;
  if (debug.stage === 'pre_battle') {
    const sideTxt = debug.side === 'attacker' ? 'Attacker' : 'Defender';
    summary = (
      <div>
        <strong>{sideTxt} bailed pre-battle.</strong>{' '}
        <span style={labelStyle}>(opposing FP / own FP) =</span>{' '}
        <span style={numStyle}>{fmt(debug.measured_value)}</span>{' '}
        <span style={labelStyle}>&gt; threshold</span>{' '}
        <span style={numStyle}>{fmt(debug.threshold)}</span>
      </div>
    );
  } else if (debug.stage === 'mid_battle') {
    const sideTxt = debug.side === 'attacker' ? 'Attacker' : 'Defender';
    summary = (
      <div>
        <strong>{sideTxt} retreated mid-battle (round {debug.round}).</strong>{' '}
        <span style={labelStyle}>FP loss =</span>{' '}
        <span style={numStyle}>{(debug.measured_value * 100).toFixed(0)}%</span>{' '}
        <span style={labelStyle}>&gt; threshold</span>{' '}
        <span style={numStyle}>{(debug.threshold * 100).toFixed(0)}%</span>
      </div>
    );
  } else {
    summary = (
      <div>
        <strong>No retreat fired.</strong>{' '}
        <span style={labelStyle}>Battle resolved over {debug.round} round(s).</span>
      </div>
    );
  }

  return (
    <div style={{ fontSize: 12, color: '#dcd6c4', display: 'flex', flexDirection: 'column', gap: 4 }}>
      {summary}
      <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', columnGap: 8, rowGap: 2 }}>
        <span style={labelStyle}>Pre-battle ratios:</span>
        <span>
          <span style={numStyle}>atk {fmt(debug.attacker_prebattle_ratio)}</span>{' '}
          <span style={labelStyle}>(thr {fmt(debug.attacker_prebattle_threshold)})</span>
          {' · '}
          <span style={numStyle}>def {fmt(debug.defender_prebattle_ratio)}</span>{' '}
          <span style={labelStyle}>(thr {fmt(debug.defender_prebattle_threshold)})</span>
        </span>
        <span style={labelStyle}>Effective FP:</span>
        <span>
          <span style={numStyle}>atk {fmt(battle.attacker_initial_fp)}</span>
          {' · '}
          <span style={numStyle}>def {fmt(battle.defender_initial_fp)}</span>
          <div style={{ ...labelStyle, marginTop: 2 }}>
            Includes per-unit DEF stat × (1 + terrain
            {battle.terrain ? ` ${battle.terrain}` : ''}) × (1 + fort
            {battle.fort_level > 0 ? ` L${battle.fort_level}` : ' L0'})
            × general bonus, + 8 FP per defending militia (entrenchment).
            That's why this is much larger than the sum of unit FP shown below.
          </div>
        </span>
      </div>
    </div>
  );
}

// ── Per-side force column for LandBattleDetails ─────────────────
function ForceColumn({
  side, role, flag, initial, survivedCount, survivors, casualties, unitLogs, showFirepower,
}: {
  side: string;
  role: 'Attacker' | 'Defender';
  flag?: string;
  initial: number;
  survivedCount: number;
  survivors: BattleUnit[];
  casualties: string[];
  unitLogs?: BattleUnitLog[];
  showFirepower?: boolean;
}) {
  // When the firepower debug toggle is on AND we have per-unit logs, render
  // every unit (alive + destroyed) from the logs in a single pass — that
  // gives us initial→final FP and the defender bonus breakdown for each
  // individual unit. When the toggle is off, fall back to the legacy
  // survivors-then-casualties layout with FP hidden so the screen reads
  // as a clean roster.
  const useLogs = !!showFirepower && unitLogs && unitLogs.length > 0;
  return (
    <div style={styles.forceCol}>
      <div style={{ ...styles.forceHeader, display: 'flex', alignItems: 'center', gap: 6 }}>
        {flag && <Flag svg={flag} width={20} height={13} />}
        {side}
        <span style={styles.roleTag}> ({role})</span>
      </div>
      <div style={styles.forceStats}>
        {initial} engaged &middot; {survivedCount} survived &middot; {casualties.length} lost
      </div>
      <div style={{ marginTop: 4 }}>
        {useLogs ? (
          unitLogs!.map((log, i) => {
            const destroyed = log.final_health === 0;
            const suffix = log.defender_breakdown
              ? <DefenderBreakdownLine breakdown={log.defender_breakdown} />
              : (role === 'Attacker'
                  ? <AttackerModifierLine log={log} />
                  : undefined);
            return (
              <UnitRow
                key={`l${i}`}
                unit_type={log.unit_type}
                medals={destroyed ? log.medals_initial : log.medals_final}
                health={log.final_health}
                effective_firepower={log.final_firepower}
                initialFirepower={log.initial_firepower}
                showFirepower={true}
                fpSuffix={suffix}
                destroyed={destroyed}
              />
            );
          })
        ) : (
          <>
            {survivors.map((u, i) => (
              <UnitRow
                key={`s${i}`}
                unit_type={u.unit_type}
                medals={u.medals}
                health={u.health}
                effective_firepower={u.effective_firepower}
                showFirepower={!!showFirepower}
              />
            ))}
            {casualties.map((t, i) => (
              <UnitRow
                key={`c${i}`}
                unit_type={t}
                medals={0}
                health={0}
                effective_firepower={0}
                showFirepower={!!showFirepower}
                destroyed
              />
            ))}
          </>
        )}
        {survivors.length === 0 && casualties.length === 0 && (
          <div style={{ color: '#888', fontStyle: 'italic', fontSize: 'var(--ui-font-size, 14px)' }}>No units recorded</div>
        )}
      </div>
    </div>
  );
}

// ── Attacker per-unit modifier annotation ───────────────────────
// Tells the user *why* an attacker unit's FP looks the way it does.
// FPN (raw firepower) lives in units.lua; the displayed `initial_firepower`
// already includes role-aware modifiers (FPM swap and ×1.25 cavalry charge
// in round 1).
function AttackerModifierLine({ log }: { log: BattleUnitLog }) {
  const cat = UNIT_TYPE_CATEGORY_FOR_HINTS[log.unit_type];
  const labelStyle: React.CSSProperties = { color: '#888', fontSize: 10, fontStyle: 'italic' };
  const parts: string[] = [];
  if (cat === 'Cavalry') {
    parts.push('round-1 FPM × 1.25 charge');
  }
  if (parts.length === 0) {
    return null;
  }
  return <div style={labelStyle}>applied: {parts.join(', ')}</div>;
}

const UNIT_TYPE_CATEGORY_FOR_HINTS: Record<string, 'Garrison' | 'Infantry' | 'Cavalry' | 'Artillery' | 'Special'> = {
  Minutemen: 'Garrison', Militia: 'Garrison', Conscript: 'Garrison', GarrisonArtillery: 'Garrison',
  Skirmishers: 'Infantry', Sharpshooters: 'Infantry', Rangers: 'Infantry',
  Regulars: 'Infantry', RifleInfantry: 'Infantry', Infantry: 'Infantry',
  Grenadiers: 'Infantry', Guards: 'Infantry', MachineGunners: 'Infantry',
  Hussars: 'Cavalry', Scouts: 'Cavalry', Carbineers: 'Cavalry', Mechanised: 'Cavalry',
  Cuirassiers: 'Cavalry', Armour: 'Cavalry',
  LightArtillery: 'Artillery', HorseArtillery: 'Artillery', FieldArtillery: 'Artillery',
  MobileArtillery: 'Artillery', Artillery: 'Artillery', SiegeArtillery: 'Artillery',
  RailroadGuns: 'Artillery',
  Sapper: 'Special', CombatEngineer: 'Special', Commandos: 'Special', Saboteur: 'Special',
  General: 'Special',
};

// ── Defender per-unit bonus breakdown ───────────────────────────
function DefenderBreakdownLine({
  breakdown,
}: {
  breakdown: NonNullable<BattleUnitLog['defender_breakdown']>;
}) {
  const fmt = (n: number) => n.toFixed(2);
  const numStyle: React.CSSProperties = { color: '#ddd', fontFamily: 'monospace' };
  const labelStyle: React.CSSProperties = { color: '#888' };
  const total = breakdown.initial_total_contribution;
  return (
    <div style={{ fontSize: 10, color: '#aab', lineHeight: 1.35 }}>
      <span style={labelStyle}>Defender contrib:</span>{' '}
      <span style={numStyle}>{fmt(total)}</span>
      <span style={labelStyle}> = fp </span>
      <span style={numStyle}>{fmt(breakdown.applied_firepower)}</span>
      <span style={labelStyle}> × fort </span>
      <span style={numStyle}>{fmt(breakdown.fort_multiplier)}</span>
      {breakdown.entrenchment_fp > 0 && (
        <>
          <span style={labelStyle}> + entrenchment </span>
          <span style={numStyle}>{fmt(breakdown.entrenchment_fp)}</span>
        </>
      )}
    </div>
  );
}

// ── 'How combat is calculated' walkthrough ──────────────────────
// Numeric walkthrough using THIS battle's values — no formulas, the
// formulas are visible per-unit above.
function CombatExplanation({ battle }: { battle: LandBattleData }) {
  const fmt = (n: number) => n.toFixed(2);
  const labelStyle: React.CSSProperties = { color: '#aab' };
  const numStyle: React.CSSProperties = { color: '#fff', fontFamily: 'monospace' };
  const subStyle: React.CSSProperties = { color: '#888', fontSize: 11 };
  const rowStyle: React.CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'minmax(140px, auto) 1fr',
    columnGap: 12, rowGap: 2, fontSize: 11,
  };

  const atkLogs = battle.attacker_unit_logs;
  const defLogs = battle.defender_unit_logs;
  const atkSum = atkLogs.reduce((s, u) => s + u.initial_firepower, 0);
  const defSum = defLogs.reduce((s, u) => s + (u.defender_breakdown?.initial_total_contribution ?? u.initial_firepower), 0);
  const atkGenBonus = atkSum > 0 ? battle.attacker_initial_fp / atkSum : 1;
  const defGenBonus = defSum > 0 ? battle.defender_initial_fp / defSum : 1;

  // Range first-strike: figure out max ranges from unit type stats.
  const atkRanges = atkLogs.map(u => UNIT_RANGE[u.unit_type] ?? 0);
  const defRanges = defLogs.map(u => UNIT_RANGE[u.unit_type] ?? 0);
  const atkMaxR = atkRanges.length > 0 ? Math.max(...atkRanges) : 0;
  const defMaxR = defRanges.length > 0 ? Math.max(...defRanges) : 0;
  const firstStrikeSide = atkMaxR > defMaxR ? 'attacker' : defMaxR > atkMaxR ? 'defender' : null;
  const firstStrikeOpp = firstStrikeSide === 'attacker' ? defMaxR : atkMaxR;
  const firstStrikeUnits = firstStrikeSide === 'attacker'
    ? atkLogs.filter(u => (UNIT_RANGE[u.unit_type] ?? 0) > defMaxR)
    : firstStrikeSide === 'defender'
      ? defLogs.filter(u => (UNIT_RANGE[u.unit_type] ?? 0) > atkMaxR)
      : [];
  const firstStrikeFp = firstStrikeUnits.reduce((s, u) => s + u.initial_firepower, 0);

  return (
    <div style={{ fontSize: 12, color: '#dcd6c4', display: 'flex', flexDirection: 'column', gap: 12 }}>
      {/* Attacker walkthrough */}
      <div>
        <div style={{ marginBottom: 4 }}>
          <strong>Attacker initial FP:</strong>{' '}
          <span style={numStyle}>{fmt(battle.attacker_initial_fp)}</span>{' '}
          <span style={subStyle}>— each unit's contribution is its applied firepower (FPN × medals × health, plus FPM swap and ×1.25 charge for round-1 cavalry).</span>
        </div>
        <div style={rowStyle}>
          {atkLogs.map((u, i) => (
            <React.Fragment key={`a${i}`}>
              <span style={labelStyle}>{splitTitle(u.unit_type)}:</span>
              <span>
                <span style={numStyle}>{fmt(u.initial_firepower)}</span>
              </span>
            </React.Fragment>
          ))}
          <span style={labelStyle}>Sum × general bonus:</span>
          <span>
            <span style={numStyle}>{fmt(atkSum)}</span>
            <span style={subStyle}> × </span>
            <span style={numStyle}>{fmt(atkGenBonus)}</span>
            <span style={subStyle}> = </span>
            <span style={numStyle}>{fmt(battle.attacker_initial_fp)}</span>
          </span>
        </div>
      </div>

      {/* Defender walkthrough */}
      <div>
        <div style={{ marginBottom: 4 }}>
          <strong>Defender initial FP:</strong>{' '}
          <span style={numStyle}>{fmt(battle.defender_initial_fp)}</span>{' '}
          <span style={subStyle}>— each unit's contribution = applied_fp × fort + entrenchment (Garrison units in the province for ≥1 turn).</span>
        </div>
        <div style={rowStyle}>
          {defLogs.map((u, i) => {
            const b = u.defender_breakdown;
            const contrib = b ? b.initial_total_contribution : u.initial_firepower;
            return (
              <React.Fragment key={`d${i}`}>
                <span style={labelStyle}>{splitTitle(u.unit_type)}:</span>
                <span>
                  <span style={numStyle}>{fmt(contrib)}</span>
                  {b && (
                    <span style={subStyle}>
                      {' '}({fmt(b.applied_firepower)} × {fmt(b.fort_multiplier)}
                      {b.entrenchment_fp > 0 && ` + ${fmt(b.entrenchment_fp)}`})
                    </span>
                  )}
                </span>
              </React.Fragment>
            );
          })}
          <span style={labelStyle}>Sum × general bonus:</span>
          <span>
            <span style={numStyle}>{fmt(defSum)}</span>
            <span style={subStyle}> × </span>
            <span style={numStyle}>{fmt(defGenBonus)}</span>
            <span style={subStyle}> = </span>
            <span style={numStyle}>{fmt(battle.defender_initial_fp)}</span>
          </span>
        </div>
      </div>

      {/* First-strike volley */}
      <div>
        <div style={{ marginBottom: 4 }}>
          <strong>Range first-strike:</strong>{' '}
          <span style={subStyle}>
            attacker max range <span style={numStyle}>{atkMaxR}</span>{' '}
            vs defender max range <span style={numStyle}>{defMaxR}</span>.
          </span>
        </div>
        {firstStrikeSide === null ? (
          <div style={subStyle}>No first-strike volley fired (ranges are equal).</div>
        ) : (
          <div style={subStyle}>
            {firstStrikeSide === 'attacker' ? 'Attacker' : 'Defender'} fires one free volley before round 1
            with {firstStrikeUnits.length} over-range unit{firstStrikeUnits.length === 1 ? '' : 's'}{' '}
            (range &gt; <span style={numStyle}>{firstStrikeOpp}</span>),{' '}
            volley FP <span style={numStyle}>{fmt(firstStrikeFp)}</span>.
            {firstStrikeUnits.length === 0 && ' (none qualified.)'}
          </div>
        )}
      </div>

      {/* Round-by-round damage exchange */}
      <div>
        <strong>Damage exchange (each round):</strong>{' '}
        <span style={subStyle}>
          Each unit picks one enemy target and concentrates its firepower on it.
          Front-line units (infantry / cavalry / garrison) target the enemy
          front-line first, falling through to artillery only if the front-line
          is wiped. Artillery targets enemy artillery first, falling through to
          front-line. Damage spills to the next priority target on overkill, so
          a stack always finishes off wounded units before the next one.
          Up to 10 rounds; ends early on wipeout or FP-loss retreat.
        </span>
      </div>
    </div>
  );
}

function splitTitle(s: string): string {
  return s.replace(/([A-Z])/g, ' $1').trim();
}

// ── 'How the battle played out' per-round table ─────────────────
// Renders the BattleRoundLog trace from the resolver: optional first-strike
// volley as round 0, then each combat round with side total FP, shots
// fired, and casualties. Gated on the same `showFirepower` toggle as
// CombatExplanation.
function RoundPlayout({ battle }: { battle: LandBattleData }) {
  const fmt = (n: number) => n.toFixed(2);
  const numStyle: React.CSSProperties = { color: '#fff', fontFamily: 'monospace' };
  const labelStyle: React.CSSProperties = { color: '#aab' };
  const subStyle: React.CSSProperties = { color: '#888', fontSize: 11 };
  const atkColor = '#9ecbff';
  const defColor = '#ffb38a';

  const renderCasualties = (cs: string[]) => {
    if (cs.length === 0) return <span style={subStyle}>—</span>;
    return <span style={numStyle}>{cs.map(splitTitle).join(', ')}</span>;
  };

  return (
    <div style={{ fontSize: 12, color: '#dcd6c4', display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={subStyle}>
        Per-round trace from the resolver. Each shot picks one priority
        target (front-line shooters target enemy front-line; artillery
        targets enemy artillery) and damage spills to the next on overkill.
      </div>
      {battle.round_logs.map((r, i) => {
        const isVolley = r.round === 0;
        const title = isVolley
          ? `First-strike volley — ${r.first_strike_side === 'attacker' ? 'attacker' : 'defender'} fires`
          : `Round ${r.round}`;
        return (
          <div
            key={i}
            style={{
              borderLeft: `3px solid ${isVolley ? '#d4a52a' : '#555'}`,
              paddingLeft: 8,
            }}
          >
            <div style={{ marginBottom: 4 }}>
              <strong>{title}</strong>
              {r.retreat_triggered && (
                <span style={{ color: '#ffb38a', marginLeft: 6 }}>
                  → {r.retreat_triggered} retreats (FP loss past threshold; +10% damage on the way out)
                </span>
              )}
            </div>
            {isVolley ? (
              <div style={{ display: 'grid', gridTemplateColumns: 'minmax(140px, auto) 1fr', columnGap: 12, rowGap: 2 }}>
                <span style={labelStyle}>Volley FP:</span>
                <span>
                  <span style={numStyle}>
                    {fmt(r.first_strike_side === 'attacker' ? r.atk_fp : r.def_fp)}
                  </span>
                  <span style={subStyle}> from </span>
                  <span style={numStyle}>
                    {r.first_strike_side === 'attacker' ? r.atk_shots : r.def_shots}
                  </span>
                  <span style={subStyle}> over-range shooter(s)</span>
                </span>
                <span style={labelStyle}>Casualties:</span>
                <span>
                  {r.first_strike_side === 'attacker'
                    ? renderCasualties(r.def_casualties)
                    : renderCasualties(r.atk_casualties)}
                </span>
              </div>
            ) : (
              <div style={{ display: 'grid', gridTemplateColumns: 'minmax(110px, auto) 1fr', columnGap: 12, rowGap: 2 }}>
                <span style={{ ...labelStyle, color: atkColor }}>Attacker fire:</span>
                <span>
                  <span style={numStyle}>{fmt(r.atk_fp)}</span>
                  <span style={subStyle}> from </span>
                  <span style={numStyle}>{r.atk_shots}</span>
                  <span style={subStyle}> shooter{r.atk_shots === 1 ? '' : 's'}</span>
                </span>
                <span style={{ ...labelStyle, color: defColor }}>Defender fire:</span>
                <span>
                  <span style={numStyle}>{fmt(r.def_fp)}</span>
                  <span style={subStyle}> from </span>
                  <span style={numStyle}>{r.def_shots}</span>
                  <span style={subStyle}> shooter{r.def_shots === 1 ? '' : 's'}</span>
                </span>
                <span style={labelStyle}>Atk casualties:</span>
                <span>{renderCasualties(r.atk_casualties)}</span>
                <span style={labelStyle}>Def casualties:</span>
                <span>{renderCasualties(r.def_casualties)}</span>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// Keep in sync with scripts/config/units.lua. Used only by CombatExplanation
// to show range info; the resolver itself reads canonical stats from Rust.
const UNIT_RANGE: Record<string, number> = {
  Minutemen: 1, Militia: 2, Conscript: 2, GarrisonArtillery: 3,
  Skirmishers: 1, Sharpshooters: 3, Rangers: 5,
  Regulars: 1, RifleInfantry: 2, Infantry: 2,
  Grenadiers: 1, Guards: 2, MachineGunners: 2,
  Hussars: 1, Scouts: 1, Carbineers: 2, Mechanised: 4,
  Cuirassiers: 1, Armour: 6,
  LightArtillery: 3, HorseArtillery: 4, FieldArtillery: 5, MobileArtillery: 5,
  Artillery: 4, SiegeArtillery: 6, RailroadGuns: 17,
  Sapper: 1, CombatEngineer: 2, Commandos: 2, Saboteur: 1,
  General: 0,
};

// ── Naval battle details sub-component ──────────────────────────
function NavalBattleDetails({ battle, flagById }: { battle: NavalBattleData; flagById: Record<number, string> }) {
  const winnerName = battle.attacker_won ? battle.attacker : battle.defender;
  const winnerId = battle.attacker_won ? battle.attacker_id : battle.defender_id;
  const winnerFlag = flagById[winnerId];

  return (
    <div style={styles.detailsPanel}>
      <div style={{
        ...styles.outcomeBanner,
        borderLeftColor: battle.attacker_won ? '#2ecc40' : '#e63946',
      }}>
        <div style={{ ...styles.outcomeTitle, display: 'flex', alignItems: 'center', gap: 8 }}>
          {winnerFlag && <Flag svg={winnerFlag} width={24} height={16} />}
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
            <div style={{ ...styles.forceHeader, display: 'flex', alignItems: 'center', gap: 6 }}>
              {flagById[battle.attacker_id] && <Flag svg={flagById[battle.attacker_id]} width={20} height={13} />}
              {battle.attacker}
            </div>
            <div style={styles.forceStats}>{battle.attacker_survivors_count} ships survived</div>
            {battle.attacker_ships_lost.length > 0 && (
              <div style={styles.casualties}>
                <span style={styles.casualtyLabel}>Lost: </span>
                {formatCasualties(battle.attacker_ships_lost)}
              </div>
            )}
          </div>
          <div style={styles.forceCol}>
            <div style={{ ...styles.forceHeader, display: 'flex', alignItems: 'center', gap: 6 }}>
              {flagById[battle.defender_id] && <Flag svg={flagById[battle.defender_id]} width={20} height={13} />}
              {battle.defender}
            </div>
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
  turnLabel: { color: '#aaa', fontSize: 'var(--ui-font-size, 14px)' },
  modeTabs: { display: 'flex', gap: 4, marginLeft: 'auto' },
  modeTab: {
    padding: '4px 12px', background: '#1a1a2e', color: '#888',
    border: '1px solid #3a3520', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)',
  },
  modeTabActive: {
    padding: '4px 12px', background: '#3a3520', color: '#daa520',
    border: '1px solid #5a5030', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)',
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
    border: 'none', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)',
    textAlign: 'left' as const,
  },
  archiveItemActive: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    width: '100%', padding: '6px 12px', background: '#2a2a4e', color: '#daa520',
    border: 'none', cursor: 'pointer', fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)',
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
    fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)', textAlign: 'left' as const,
  },
  battleItemActive: {
    display: 'flex', alignItems: 'center', gap: 8,
    width: '100%', padding: '8px 12px', background: '#2a2a4e', color: '#daa520',
    border: 'none', borderBottom: '1px solid #2a2a3e', cursor: 'pointer',
    fontFamily: "'Georgia', serif", fontSize: 'var(--ui-font-size, 14px)', textAlign: 'left' as const,
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
    fontSize: 'var(--ui-font-size, 14px)', color: '#aaa', marginTop: 4,
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
    fontSize: 'var(--ui-font-size, 14px)', fontWeight: 'bold', color: '#e0d8c0', marginBottom: 4,
  },
  roleTag: {
    fontSize: 11, color: '#888', fontWeight: 'normal',
  },
  forceStats: {
    fontSize: 'var(--ui-font-size, 14px)', color: '#bbb', marginBottom: 2,
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
    fontSize: 'var(--ui-font-size, 14px)', color: '#bbb',
  },
  fieldLabel: {
    color: '#aaa',
  },
  siegeNote: {
    color: '#daa520', fontSize: 11,
  },
  medalItem: {
    fontSize: 'var(--ui-font-size, 14px)', color: '#daa520', marginBottom: 2,
  },
};
