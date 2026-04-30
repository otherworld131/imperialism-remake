import { useState, useEffect, useMemo } from 'react';
import {
  getScenarios, newGame, newScenarioGame, getMapData,
  newObserverGame, newObserverScenarioGame, setHumanPlayer,
  applyFlavor,
  DEFAULT_MAP_GEN_CONFIG,
  parseGameJson,
} from '../wasm';
import type { TileData, MapMode, MapGenConfig } from '../wasm';
import HexMap from './HexMap';
import Flag from './Flag';

const MAP_SIZE_PRESETS: Array<{ key: string; label: string; width: number; height: number }> = [
  { key: 'small', label: 'Small (60×40)', width: 60, height: 40 },
  { key: 'medium', label: 'Medium (80×50)', width: 80, height: 50 },
  { key: 'large', label: 'Large (120×70)', width: 120, height: 70 },
];

const NATION_COLORS: Record<string, string> = {
  Yellow: '#ffd900', Orange: '#ff8c00', LightBlue: '#66b3ff',
  Red: '#e62626', Green: '#1abf1a', Purple: '#a633d9',
  Blue: '#3359e6', Gray: '#999', Brown: '#8c5926',
  Pink: '#ff80b3', Teal: '#00b3a6', Olive: '#808000',
};

const DIFFICULTIES = ['Introductory', 'Easy', 'Normal', 'Hard', 'NOI'];

export interface GameStartParams {
  mapKey: string;
  observerMode: boolean;
  scenario: string | null;
  difficulty: number;
  nationIdx: number;
  mapGenConfig: MapGenConfig;
  organicBorders: boolean;
  hideHexGrid: boolean;
}

interface Props {
  onStartGame: (gameJson: string, params: GameStartParams) => void;
}

interface GpInfo {
  idx: number;
  id: number;
  name: string;
  color: string;
  governmentTitle: string;
  flagSvg: string;
}

type Step = 'config' | 'preview';

function randomSeed(): string {
  return Math.random().toString(36).slice(2, 10);
}

export default function GameSetup({ onStartGame }: Props) {
  const [scenarios, setScenarios] = useState<any[]>([]);
  const [selectedScenario, setSelectedScenario] = useState<string | null>(null);
  const [difficulty, setDifficulty] = useState(2);
  const [mapKey, setMapKey] = useState('');
  const [flavorKey, setFlavorKey] = useState('');
  const [observerMode, setObserverMode] = useState(true);
  const [organicBorders, setOrganicBorders] = useState(true);
  const [hideHexGrid, setHideHexGrid] = useState(true);

  // Random-map customization (ignored for historical scenarios).
  const [mapWidth, setMapWidth] = useState(DEFAULT_MAP_GEN_CONFIG.width);
  const [mapHeight, setMapHeight] = useState(DEFAULT_MAP_GEN_CONFIG.height);
  const [numGreatPowers, setNumGreatPowers] = useState(DEFAULT_MAP_GEN_CONFIG.numGreatPowers);
  const [numMinorNations, setNumMinorNations] = useState(DEFAULT_MAP_GEN_CONFIG.numMinorNations);
  const [showAdvancedSize, setShowAdvancedSize] = useState(false);

  const mapGenConfig: MapGenConfig = useMemo(
    () => ({ width: mapWidth, height: mapHeight, numGreatPowers, numMinorNations }),
    [mapWidth, mapHeight, numGreatPowers, numMinorNations],
  );
  const activePreset = MAP_SIZE_PRESETS.find(p => p.width === mapWidth && p.height === mapHeight);

  const applyPreset = (preset: typeof MAP_SIZE_PRESETS[number]) => {
    setMapWidth(preset.width);
    setMapHeight(preset.height);
  };

  const [step, setStep] = useState<Step>('config');
  const [previewJson, setPreviewJson] = useState<string>('');
  const [previewTiles, setPreviewTiles] = useState<TileData[]>([]);
  const [previewGps, setPreviewGps] = useState<GpInfo[]>([]);
  const [pickedNationIdx, setPickedNationIdx] = useState<number | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewMapMode, setPreviewMapMode] = useState<'terrain' | 'political'>('political');

  useEffect(() => {
    getScenarios().then(setScenarios).catch(() => { /* no scenarios available */ });
  }, []);

  const effectiveMapKey = useMemo(() => mapKey || 'imperialism', [mapKey]);

  const extractGps = (parsed: any): GpInfo[] =>
    (parsed.nations as any[])
      .filter((n: any) => n.nation_type === 'GreatPower')
      .map((n: any, idx: number) => ({
        idx,
        id: n.id,
        name: n.name,
        color: n.color,
        governmentTitle: n.government_title || n.name,
        flagSvg: n.flag_svg || '',
      }));

  const buildPreview = async (keyOverride?: string, flavorOverride?: string) => {
    setPreviewError(null);
    const key = keyOverride ?? effectiveMapKey;
    const fkey = flavorOverride ?? flavorKey;
    try {
      const json = selectedScenario
        ? await newScenarioGame(selectedScenario, difficulty, 0, fkey)
        : await newGame(key, difficulty, 0, mapGenConfig, fkey);
      // Detect error payloads from the bridge.
      const parsed = parseGameJson(json);
      if (parsed.error) {
        setPreviewError(parsed.error);
        return;
      }
      const gps = extractGps(parsed);
      const tiles = await getMapData(json, true);
      setPreviewJson(json);
      setPreviewTiles(tiles);
      setPreviewGps(gps);
      // Carry over previous pick if valid.
      if (pickedNationIdx != null && pickedNationIdx >= gps.length) {
        setPickedNationIdx(null);
      }
      setStep('preview');
    } catch (e) {
      setPreviewError(String(e));
    }
  };

  const handleReroll = () => {
    const fresh = randomSeed();
    setFlavorKey(fresh);
    if (selectedScenario) {
      buildPreview(undefined, fresh);
    } else {
      setMapKey(fresh);
      buildPreview(fresh, fresh);
    }
  };

  const handleRerollNames = async () => {
    if (!previewJson) return;
    const fresh = randomSeed();
    setFlavorKey(fresh);
    try {
      const updated = await applyFlavor(previewJson, fresh);
      const parsed = parseGameJson(updated);
      if (parsed.error) {
        setPreviewError(parsed.error);
        return;
      }
      setPreviewJson(updated);
      setPreviewGps(extractGps(parsed));
      // Tiles use Nation.name in the legend; refresh them too so labels
      // pick up the new short names.
      const tiles = await getMapData(updated, true);
      setPreviewTiles(tiles);
    } catch (e) {
      setPreviewError(String(e));
    }
  };

  const handleTileClick = (tile: TileData) => {
    if (tile.nation_id == null) return;
    const gp = previewGps.find(g => g.id === tile.nation_id);
    if (gp) setPickedNationIdx(gp.idx);
  };

  const handleBegin = async () => {
    const idx = pickedNationIdx ?? 0;
    let gameJson: string;
    if (observerMode) {
      gameJson = selectedScenario
        ? await newObserverScenarioGame(selectedScenario, difficulty, flavorKey)
        : await newObserverGame(effectiveMapKey, difficulty, mapGenConfig, flavorKey);
      if (idx !== 0) {
        gameJson = await setHumanPlayer(gameJson, idx);
      }
    } else {
      gameJson = previewJson;
      if (idx !== 0) {
        gameJson = await setHumanPlayer(gameJson, idx);
      }
    }
    onStartGame(gameJson, {
      mapKey: effectiveMapKey,
      observerMode,
      scenario: selectedScenario,
      difficulty,
      nationIdx: idx,
      mapGenConfig,
      organicBorders,
      hideHexGrid,
    });
  };

  const pickedGp = pickedNationIdx != null ? previewGps[pickedNationIdx] : null;
  const canBegin = observerMode || pickedNationIdx != null;

  if (step === 'config') {
    return (
      <div style={s.page}>
        <div style={s.container}>
          <div style={s.header}>
            <h1 style={s.headerTitle}>Imperialism</h1>
            <p style={s.headerSub}>A game of diplomacy, trade, and conquest in the age of empire</p>
          </div>

          <div style={s.body}>
            <div style={s.group}>
              <label style={s.label}>Scenario</label>
              <div style={s.cards}>
                <div
                  style={selectedScenario === null ? { ...s.card, ...s.cardSelected } : s.card}
                  onClick={() => setSelectedScenario(null)}
                >
                  <div style={s.cardIcon}>&#127758;</div>
                  <div style={s.cardName}>Random Map</div>
                  <div style={s.cardDesc}>Procedurally generated world</div>
                </div>
                {scenarios.map((sc: any) => (
                  <div
                    key={sc.id}
                    style={selectedScenario === sc.id ? { ...s.card, ...s.cardSelected } : s.card}
                    onClick={() => setSelectedScenario(sc.id)}
                  >
                    <div style={s.cardIcon}>&#128214;</div>
                    <div style={s.cardName}>{sc.name || sc.id}</div>
                    <div style={s.cardDesc}>{sc.description || `Year ${sc.year || '?'}`}</div>
                  </div>
                ))}
              </div>
            </div>

            <div style={s.group}>
              <label style={s.label}>Difficulty</label>
              <div style={s.diffRow}>
                {DIFFICULTIES.map((d, i) => (
                  <div
                    key={d}
                    style={difficulty === i ? { ...s.diffBtn, ...s.diffSelected } : s.diffBtn}
                    onClick={() => setDifficulty(i)}
                  >
                    {d}
                  </div>
                ))}
              </div>
            </div>

            {!selectedScenario && (
              <div style={s.group}>
                <label style={s.label}>Map Key (optional)</label>
                <div style={s.mapKeyRow}>
                  <input
                    style={s.mapKeyInput}
                    placeholder="Leave blank for default..."
                    maxLength={32}
                    value={mapKey}
                    onChange={e => setMapKey(e.target.value)}
                  />
                </div>
              </div>
            )}

            {!selectedScenario && (
              <div style={s.group}>
                <label style={s.label}>Map Size</label>
                <div style={s.diffRow}>
                  {MAP_SIZE_PRESETS.map(preset => (
                    <div
                      key={preset.key}
                      style={activePreset?.key === preset.key
                        ? { ...s.diffBtn, ...s.diffSelected }
                        : s.diffBtn}
                      onClick={() => applyPreset(preset)}
                    >
                      {preset.label}
                    </div>
                  ))}
                </div>
                <div style={{ marginTop: 6, fontSize: 11 }}>
                  <span
                    style={{ color: '#daa520', cursor: 'pointer', textDecoration: 'underline' }}
                    onClick={() => setShowAdvancedSize(v => !v)}
                  >
                    {showAdvancedSize ? 'Hide advanced' : 'Advanced size...'}
                  </span>
                </div>
                {showAdvancedSize && (
                  <div style={{ display: 'flex', gap: 12, marginTop: 8, alignItems: 'center' }}>
                    <label style={{ fontSize: 12, color: '#9a9a9a' }}>
                      Width:&nbsp;
                      <input
                        type="number"
                        min={30}
                        max={200}
                        step={2}
                        value={mapWidth}
                        onChange={e => setMapWidth(Math.max(30, Math.min(200, Number(e.target.value) || 0)))}
                        style={s.numInput}
                      />
                    </label>
                    <label style={{ fontSize: 12, color: '#9a9a9a' }}>
                      Height:&nbsp;
                      <input
                        type="number"
                        min={20}
                        max={150}
                        step={2}
                        value={mapHeight}
                        onChange={e => setMapHeight(Math.max(20, Math.min(150, Number(e.target.value) || 0)))}
                        style={s.numInput}
                      />
                    </label>
                  </div>
                )}
              </div>
            )}

            {!selectedScenario && (
              <div style={s.group}>
                <label style={s.label}>Nations</label>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  <div>
                    <div style={s.sliderLabelRow}>
                      <span>Great Powers</span>
                      <span style={s.sliderValue}>{numGreatPowers}</span>
                    </div>
                    <input
                      type="range"
                      min={1}
                      max={20}
                      value={numGreatPowers}
                      onChange={e => setNumGreatPowers(Number(e.target.value))}
                      style={s.slider}
                    />
                  </div>
                  <div>
                    <div style={s.sliderLabelRow}>
                      <span>Minor Nations</span>
                      <span style={s.sliderValue}>{numMinorNations}</span>
                    </div>
                    <input
                      type="range"
                      min={0}
                      max={32}
                      value={numMinorNations}
                      onChange={e => setNumMinorNations(Number(e.target.value))}
                      style={s.slider}
                    />
                  </div>
                </div>
              </div>
            )}

            <div style={s.group}>
              <label style={s.observerRow} onClick={() => setObserverMode(!observerMode)}>
                <span style={observerMode ? { ...s.observerBox, ...s.observerBoxChecked } : s.observerBox}>
                  {observerMode ? '\u2713' : ''}
                </span>
                <span>
                  <span style={s.observerLabel}>Observer Mode</span>
                  <span style={s.observerHint}> — watch AI play all {selectedScenario ? 7 : numGreatPowers} Great Powers</span>
                </span>
              </label>
            </div>

            <div style={s.group}>
              <label style={s.observerRow} onClick={() => setOrganicBorders(!organicBorders)}>
                <span style={organicBorders ? { ...s.observerBox, ...s.observerBoxChecked } : s.observerBox}>
                  {organicBorders ? '✓' : ''}
                </span>
                <span>
                  <span style={s.observerLabel}>Organic Borders</span>
                  <span style={s.observerHint}> — smooth coasts &amp; borders so the map doesn&rsquo;t look hex-shaped</span>
                </span>
              </label>
            </div>

            <div style={s.group}>
              <label style={s.observerRow} onClick={() => setHideHexGrid(!hideHexGrid)}>
                <span style={hideHexGrid ? { ...s.observerBox, ...s.observerBoxChecked } : s.observerBox}>
                  {hideHexGrid ? '✓' : ''}
                </span>
                <span>
                  <span style={s.observerLabel}>Hide Hex Grid</span>
                  <span style={s.observerHint}> — hide the faint interior hex outlines (borders still show)</span>
                </span>
              </label>
            </div>

            {previewError && <div style={s.error}>{previewError}</div>}
          </div>

          <div style={s.footer}>
            <button style={s.startBtn} onClick={() => buildPreview()}>Preview Map</button>
          </div>
        </div>
      </div>
    );
  }

  // Preview step
  return (
    <div style={s.previewPage}>
      <div style={s.previewHeader}>
        <h1 style={s.headerTitle}>Preview</h1>
        <div style={s.previewSub}>
          {selectedScenario
            ? `Scenario: ${selectedScenario}`
            : `Seed: ${effectiveMapKey}`}
          {' \u00b7 '}Names: {flavorKey || effectiveMapKey}
          {' \u00b7 '}{DIFFICULTIES[difficulty]}
          {observerMode ? ' \u00b7 Observer Mode' : ''}
        </div>
      </div>
      <div style={s.previewBody}>
        <div style={s.mapWrap}>
          <HexMap
            tiles={previewTiles}
            mapMode={previewMapMode}
            diplomacyOverlay={null}
            militaryOverlay={null}
            onMapModeChange={(mode: MapMode) => {
              if (mode === 'terrain' || mode === 'political') setPreviewMapMode(mode);
            }}
            onTileClick={handleTileClick}
            disableFogOfWar={true}
            highlightedNationId={pickedGp?.id ?? null}
            organicBorders={organicBorders}
            hideHexGrid={hideHexGrid}
            limitedMapModes={true}
          />
        </div>
        <div style={s.sidebar}>
          <div style={s.sidebarTitle}>
            {observerMode ? 'Viewpoint Nation' : 'Choose Your Empire'}
          </div>
          <div style={s.sidebarHint}>
            {observerMode
              ? 'Pick a nation whose ledger and diplomacy screens to view. You can switch in-game.'
              : 'Click a starting region on the map or a nation below.'}
          </div>
          <div style={s.gpList}>
            {previewGps.map(gp => (
              <div
                key={gp.id}
                style={pickedNationIdx === gp.idx ? { ...s.gpRow, ...s.gpRowSelected } : s.gpRow}
                onClick={() => setPickedNationIdx(gp.idx)}
              >
                <div style={{ ...s.gpSwatch, background: NATION_COLORS[gp.color] || '#888' }} />
                <Flag svg={gp.flagSvg} width={36} height={24} title={gp.governmentTitle} />
                <div style={s.gpNameBlock}>
                  <div style={s.gpName}>{gp.name}</div>
                  {gp.governmentTitle && gp.governmentTitle !== gp.name && (
                    <div style={s.gpTitle}>{gp.governmentTitle}</div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
      <div style={s.previewFooter}>
        <button style={s.secondaryBtn} onClick={() => setStep('config')}>Back</button>
        <button style={s.secondaryBtn} onClick={handleReroll}>Re-roll</button>
        <button style={s.secondaryBtn} onClick={handleRerollNames} title="Re-roll only the country names and flags. Map layout stays the same.">
          Re-roll Names
        </button>
        <div style={{ flex: 1 }} />
        <button
          style={canBegin ? s.startBtn : { ...s.startBtn, ...s.startBtnDisabled }}
          disabled={!canBegin}
          onClick={handleBegin}
        >
          Begin Campaign
        </button>
      </div>
    </div>
  );
}

const s: Record<string, React.CSSProperties> = {
  page: { fontFamily: 'Georgia, serif', background: '#1a1a2e', color: '#e0d8c0', height: '100vh', display: 'flex', justifyContent: 'center', alignItems: 'center' },
  container: { width: 700, background: '#161625', border: '2px solid #3a3520', borderRadius: 4, overflow: 'hidden' },
  header: { background: '#0f0f23', padding: '20px 30px', borderBottom: '2px solid #3a3520', textAlign: 'center' as const },
  headerTitle: { fontSize: 28, color: '#daa520', margin: 0, fontWeight: 'normal' },
  headerSub: { fontSize: 13, color: '#9a9a9a', margin: '4px 0 0' },
  body: { padding: '24px 30px' },
  group: { marginBottom: 20 },
  label: { display: 'block', fontSize: 13, color: '#daa520', marginBottom: 6, textTransform: 'uppercase' as const, letterSpacing: 0.5 },
  cards: { display: 'flex', gap: 12 },
  card: { flex: 1, padding: 14, background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 4, cursor: 'pointer', textAlign: 'center' as const },
  cardSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)' },
  cardIcon: { fontSize: 28, marginBottom: 6 },
  cardName: { fontSize: 14, fontWeight: 'bold' as const },
  cardDesc: { fontSize: 11, color: '#9a9a9a', marginTop: 4 },
  diffRow: { display: 'flex', gap: 8 },
  diffBtn: { flex: 1, padding: 8, background: '#1a1a2e', border: '1px solid #3a3520', color: '#e0d8c0', fontFamily: 'Georgia, serif', fontSize: 12, cursor: 'pointer', borderRadius: 3, textAlign: 'center' as const },
  diffSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)', color: '#daa520' },
  mapKeyRow: { display: 'flex', gap: 10, alignItems: 'center' },
  mapKeyInput: { flex: 1, padding: '6px 10px', background: '#1a1a2e', border: '1px solid #3a3520', color: '#e0d8c0', fontFamily: "'Courier New', monospace", fontSize: 13, borderRadius: 3 },
  numInput: { width: 70, padding: '4px 6px', background: '#1a1a2e', border: '1px solid #3a3520', color: '#e0d8c0', fontFamily: "'Courier New', monospace", fontSize: 12, borderRadius: 3 },
  sliderLabelRow: { display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', fontSize: 12, color: '#e0d8c0', marginBottom: 4 },
  sliderValue: { color: '#daa520', fontWeight: 'bold' as const },
  slider: { width: '100%', accentColor: '#daa520' },
  observerRow: { display: 'flex', alignItems: 'center', cursor: 'pointer', padding: '6px 0', gap: 10 },
  observerBox: { width: 16, height: 16, border: '1px solid #3a3520', background: '#1a1a2e', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 12, color: '#daa520' },
  observerBoxChecked: { borderColor: '#daa520', background: 'rgba(218,165,32,0.1)' },
  observerLabel: { fontSize: 13, color: '#e0d8c0', fontWeight: 'bold' as const },
  observerHint: { fontSize: 12, color: '#9a9a9a' },
  error: { fontSize: 12, color: '#ff6b6b', padding: 8, background: 'rgba(255,0,0,0.05)', border: '1px solid #552222' },
  footer: { padding: '16px 30px', background: '#0f0f23', borderTop: '2px solid #3a3520', display: 'flex', justifyContent: 'flex-end' },
  startBtn: { padding: '10px 40px', background: '#8b4513', color: '#fff', border: '1px solid #a0522d', fontFamily: 'Georgia, serif', fontSize: 16, fontWeight: 'bold' as const, cursor: 'pointer', borderRadius: 3, letterSpacing: 0.5 },
  startBtnDisabled: { opacity: 0.4, cursor: 'not-allowed' },
  secondaryBtn: { padding: '8px 20px', background: '#1a1a2e', color: '#e0d8c0', border: '1px solid #3a3520', fontFamily: 'Georgia, serif', fontSize: 13, cursor: 'pointer', borderRadius: 3 },

  // Preview
  previewPage: { fontFamily: 'Georgia, serif', background: '#1a1a2e', color: '#e0d8c0', height: '100vh', display: 'flex', flexDirection: 'column' as const },
  previewHeader: { background: '#0f0f23', padding: '10px 20px', borderBottom: '2px solid #3a3520', textAlign: 'center' as const },
  previewSub: { fontSize: 12, color: '#9a9a9a', marginTop: 2 },
  previewBody: { flex: 1, display: 'flex', overflow: 'hidden' },
  mapWrap: { flex: 1, position: 'relative' as const, overflow: 'hidden' },
  sidebar: { width: 320, background: '#161625', borderLeft: '2px solid #3a3520', padding: 16, overflowY: 'auto' as const },
  sidebarTitle: { fontSize: 14, color: '#daa520', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  sidebarHint: { fontSize: 11, color: '#9a9a9a', marginBottom: 14, lineHeight: 1.4 },
  gpList: { display: 'flex', flexDirection: 'column' as const, gap: 6 },
  gpRow: { display: 'flex', alignItems: 'center', gap: 10, padding: '8px 10px', background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 3, cursor: 'pointer' },
  gpRowSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)' },
  gpSwatch: { width: 12, height: 12, borderRadius: '50%', border: '1px solid rgba(255,255,255,0.2)', flexShrink: 0 },
  gpNameBlock: { display: 'flex', flexDirection: 'column' as const, minWidth: 0, flex: 1 },
  gpName: { fontSize: 13, fontWeight: 'bold' as const },
  gpTitle: { fontSize: 11, color: '#9a9a9a', marginTop: 1 },
  previewFooter: { padding: '12px 20px', background: '#0f0f23', borderTop: '2px solid #3a3520', display: 'flex', gap: 10, alignItems: 'center' },
};
