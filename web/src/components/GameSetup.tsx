import { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import {
  getScenarios, newGame, newScenarioGame, getMapData,
  newObserverGame, newObserverScenarioGame, setHumanPlayer,
  getMaxWorkersSupportable,
  applyFlavor,
  DEFAULT_MAP_GEN_CONFIG,
  DEFAULT_TERRAIN_MIX,
  parseGameJson,
} from '../wasm';
import type { CapitalOverride, TileData, MapMode, MapGenConfig, TerrainMix } from '../wasm';
import HexMap from './HexMap';
import Flag from './Flag';
import { resourceEmoji } from '../resourceEmoji';
import {
  computeNationPlacementView,
  evaluateCapitalSite,
  isValidCapitalTile,
  type CapitalSitePreview,
  tileKey,
} from './GameSetup.logic';

const MAP_SIZE_PRESETS: Array<{ key: string; label: string; width: number; height: number }> = [
  { key: 'small', label: 'Small (60×40)', width: 60, height: 40 },
  { key: 'medium', label: 'Medium (80×50)', width: 80, height: 50 },
  { key: 'large', label: 'Large (120×70)', width: 120, height: 70 },
];

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
};

const DIFFICULTIES = ['Introductory', 'Easy', 'Normal', 'Hard', 'NOI'];

export interface GameStartParams {
  mapKey: string;
  observerMode: boolean;
  scenario: string | null;
  difficulty: number;
  nationIdx: number;
  capitalOverride: CapitalOverride | null;
  mapGenConfig: MapGenConfig;
  organicBorders: boolean;
  hideHexGrid: boolean;
}

interface Props {
  onStartGame: (gameJson: string, params: GameStartParams) => void;
  onRequestLoadSavedGame?: () => void;
}

interface GpInfo {
  idx: number;
  id: number;
  name: string;
  color: string;
  governmentTitle: string;
  flagSvg: string;
}

interface SuggestedPlacement extends CapitalSitePreview {
  provinceId: number | null;
  provinceName: string;
}

type Step = 'config' | 'preview';
type PreviewStage = 'nation' | 'capital';

function randomSeed(): string {
  return Math.random().toString(36).slice(2, 10);
}

export default function GameSetup({ onStartGame, onRequestLoadSavedGame }: Props) {
  const [scenarios, setScenarios] = useState<any[]>([]);
  const [selectedScenario, setSelectedScenario] = useState<string | null>(null);
  const [difficulty, setDifficulty] = useState(2);
  const [mapKey, setMapKey] = useState('');
  const [flavorKey, setFlavorKey] = useState('');
  const [observerMode, setObserverMode] = useState(true);
  const [organicBorders, setOrganicBorders] = useState(true);
  const [hideHexGrid, setHideHexGrid] = useState(true);

  const [mapWidth, setMapWidth] = useState(DEFAULT_MAP_GEN_CONFIG.width);
  const [mapHeight, setMapHeight] = useState(DEFAULT_MAP_GEN_CONFIG.height);
  const [numGreatPowers, setNumGreatPowers] = useState(DEFAULT_MAP_GEN_CONFIG.numGreatPowers);
  const [numMinorNations, setNumMinorNations] = useState(DEFAULT_MAP_GEN_CONFIG.numMinorNations);
  const [showAdvancedSize, setShowAdvancedSize] = useState(false);
  const [terrainMix, setTerrainMix] = useState<TerrainMix>(DEFAULT_TERRAIN_MIX);

  const mapGenConfig: MapGenConfig = useMemo(
    () => ({ width: mapWidth, height: mapHeight, numGreatPowers, numMinorNations, terrain: terrainMix }),
    [mapWidth, mapHeight, numGreatPowers, numMinorNations, terrainMix],
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
  const [previewStage, setPreviewStage] = useState<PreviewStage>('nation');
  const [hoveredCapital, setHoveredCapital] = useState<CapitalSitePreview | null>(null);
  const [pickedCapital, setPickedCapital] = useState<CapitalSitePreview | null>(null);
  const [sidebarHoveredCapital, setSidebarHoveredCapital] = useState<SuggestedPlacement | null>(null);
  const [suggestedCapitals, setSuggestedCapitals] = useState<SuggestedPlacement[]>([]);
  const [placementScale, setPlacementScale] = useState<number | undefined>(undefined);
  const [placementOffset, setPlacementOffset] = useState<{ x: number; y: number } | undefined>(undefined);
  const [mapViewport, setMapViewport] = useState({ width: 0, height: 0 });
  const mapWrapRef = useRef<HTMLDivElement>(null);
  const capitalSupportCacheRef = useRef(new Map<string, number>());
  const hoveredCapitalSeqRef = useRef(0);
  const pickedCapitalSeqRef = useRef(0);
  const suggestionsSeqRef = useRef(0);

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

  const resetPlacementState = useCallback(() => {
    hoveredCapitalSeqRef.current += 1;
    pickedCapitalSeqRef.current += 1;
    suggestionsSeqRef.current += 1;
    capitalSupportCacheRef.current.clear();
    setPreviewStage('nation');
    setHoveredCapital(null);
    setPickedCapital(null);
    setSidebarHoveredCapital(null);
    setSuggestedCapitals([]);
    setPlacementScale(undefined);
    setPlacementOffset(undefined);
  }, []);

  const resolveCapitalSupport = useCallback(async (preview: CapitalSitePreview): Promise<CapitalSitePreview> => {
    const key = tileKey(preview.capital.q, preview.capital.r);
    const cached = capitalSupportCacheRef.current.get(key);
    if (cached != null) return { ...preview, support: cached };
    const support = await getMaxWorkersSupportable(
      preview.foodSupply.grain,
      preview.foodSupply.fruit,
      preview.foodSupply.meat,
    );
    capitalSupportCacheRef.current.set(key, support);
    return { ...preview, support };
  }, []);

  const buildPreview = useCallback(async (keyOverride?: string, flavorOverride?: string) => {
    setPreviewError(null);
    const key = keyOverride ?? effectiveMapKey;
    const fkey = flavorOverride ?? flavorKey;
    try {
      const json = selectedScenario
        ? await newScenarioGame(selectedScenario, difficulty, 0, fkey)
        : await newGame(key, difficulty, 0, mapGenConfig, fkey);
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
      setPreviewMapMode('political');
      resetPlacementState();
      if (pickedNationIdx != null && pickedNationIdx >= gps.length) {
        setPickedNationIdx(null);
      }
      setStep('preview');
    } catch (e) {
      setPreviewError(String(e));
    }
    // pickedNationIdx intentionally omitted: we only consult it to clear an
    // out-of-range pick, which is fine to skip when the pick is stale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [effectiveMapKey, flavorKey, selectedScenario, difficulty, mapGenConfig, resetPlacementState]);

  const skipNextTerrainRebuild = useRef(true);
  useEffect(() => {
    if (step !== 'preview') {
      skipNextTerrainRebuild.current = true;
      return;
    }
    if (skipNextTerrainRebuild.current) {
      skipNextTerrainRebuild.current = false;
      return;
    }
    const t = setTimeout(() => { buildPreview(); }, 200);
    return () => clearTimeout(t);
  }, [terrainMix, step, buildPreview]);

  const randomizeTerrain = useCallback(() => {
    const rand = (lo: number, hi: number) => lo + Math.random() * (hi - lo);
    const hardMargin = Math.round(rand(0, 3));
    const falloff = Math.round(rand(hardMargin + 2, 12));
    setTerrainMix({
      grassland: rand(20, 70),
      forest: rand(5, 35),
      hills: rand(3, 25),
      mountain: rand(2, 18),
      desert: rand(0, 18),
      swamp: rand(0, 12),
      tundra: rand(0, 10),
      forest_cluster: Math.round(rand(10, 50)),
      hills_cluster: Math.round(rand(10, 40)),
      mountain_cluster: Math.round(rand(5, 25)),
      desert_cluster: Math.round(rand(5, 30)),
      swamp_cluster: Math.round(rand(5, 25)),
      pole_tundra_strength: rand(0, 1),
      sea_hard_margin: hardMargin,
      sea_falloff_radius: falloff,
      land_amount: rand(0.5, 1.6),
      river_source_percent: Math.round(rand(0, 70)),
    });
  }, []);

  const handleReroll = () => {
    const fresh = randomSeed();
    setFlavorKey(fresh);
    setPickedNationIdx(null);
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
      const tiles = await getMapData(updated, true);
      setPreviewTiles(tiles);
      resetPlacementState();
    } catch (e) {
      setPreviewError(String(e));
    }
  };

  const tileByCoord = useMemo(() => {
    const map = new Map<string, TileData>();
    for (const tile of previewTiles) map.set(tileKey(tile.q, tile.r), tile);
    return map;
  }, [previewTiles]);

  const previewMapTiles = useMemo(() => {
    return previewTiles.map(tile => {
      if (observerMode) {
        return { ...tile, resource_hidden: false };
      }
      const wasCountryCapital = tile.is_country_capital;
      return {
        ...tile,
        resource_hidden: false,
        is_capital: false,
        is_country_capital: false,
        improvement_level: 0,
        has_depot: wasCountryCapital ? false : tile.has_depot,
        army_firepower: wasCountryCapital ? 0 : tile.army_firepower,
        army_unit_count: wasCountryCapital ? 0 : tile.army_unit_count,
        army_composition: wasCountryCapital ? null : tile.army_composition,
        naval_firepower: wasCountryCapital ? 0 : tile.naval_firepower,
        naval_ship_count: wasCountryCapital ? 0 : tile.naval_ship_count,
      };
    });
  }, [observerMode, previewTiles]);

  const pickedGp = pickedNationIdx != null ? previewGps[pickedNationIdx] : null;
  const activeCapitalPreview = sidebarHoveredCapital ?? hoveredCapital ?? pickedCapital;
  const hoveredPreviewTileKey = activeCapitalPreview ? tileKey(activeCapitalPreview.capital.q, activeCapitalPreview.capital.r) : null;
  const placedPreviewTileKey = pickedCapital ? tileKey(pickedCapital.capital.q, pickedCapital.capital.r) : null;

  useEffect(() => {
    if (observerMode) {
      resetPlacementState();
      setPreviewMapMode('political');
    }
  }, [observerMode, resetPlacementState]);

  useEffect(() => {
    if (step !== 'preview') return;
    const element = mapWrapRef.current;
    if (!element) return;
    const updateViewport = () => {
      const rect = element.getBoundingClientRect();
      setMapViewport({ width: rect.width, height: rect.height });
    };
    updateViewport();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(updateViewport);
    observer.observe(element);
    return () => observer.disconnect();
  }, [step]);

  useEffect(() => {
    if (observerMode || previewStage !== 'capital' || !pickedGp) return;
    setPreviewMapMode('terrain');
    const nextView = computeNationPlacementView(previewMapTiles, pickedGp.id, mapViewport);
    if (!nextView) return;
    setPlacementScale(nextView.scale);
    setPlacementOffset(nextView.offset);
  }, [observerMode, previewStage, pickedGp, previewMapTiles, mapViewport]);

  useEffect(() => {
    if (observerMode || previewStage !== 'capital' || !pickedGp) {
      suggestionsSeqRef.current += 1;
      setSidebarHoveredCapital(null);
      setSuggestedCapitals([]);
      return;
    }
    const seq = suggestionsSeqRef.current + 1;
    suggestionsSeqRef.current = seq;
    setSidebarHoveredCapital(null);

    const candidates = previewMapTiles
      .filter(tile => isValidCapitalTile(tile, pickedGp.id))
      .map(tile => ({
        tile,
        preview: evaluateCapitalSite(tile, tileByCoord, pickedGp.id),
      }))
      .filter((entry): entry is { tile: TileData; preview: CapitalSitePreview } => entry.preview != null);

    void Promise.all(
      candidates.map(async ({ tile, preview }) => {
        const resolved = await resolveCapitalSupport(preview);
        return {
          ...resolved,
          provinceId: tile.province_id,
          provinceName: tile.province || 'Unknown Province',
        } satisfies SuggestedPlacement;
      }),
    ).then(resolved => {
      if (suggestionsSeqRef.current !== seq) return;
      const score = (entry: SuggestedPlacement) =>
        (entry.support ?? 0) * 1000 + entry.resources.reduce((sum, resource) => sum + resource.amount, 0);
      const ranked = resolved.sort((a, b) => {
        const supportDelta = (b.support ?? 0) - (a.support ?? 0);
        if (supportDelta !== 0) return supportDelta;
        const yieldDelta = b.resources.reduce((sum, resource) => sum + resource.amount, 0)
          - a.resources.reduce((sum, resource) => sum + resource.amount, 0);
        if (yieldDelta !== 0) return yieldDelta;
        const scoreDelta = score(b) - score(a);
        if (scoreDelta !== 0) return scoreDelta;
        if (a.provinceName !== b.provinceName) return a.provinceName.localeCompare(b.provinceName);
        if (a.capital.q !== b.capital.q) return a.capital.q - b.capital.q;
        return a.capital.r - b.capital.r;
      });
      setSuggestedCapitals(ranked.slice(0, 5));
    }).catch(() => {
      if (suggestionsSeqRef.current !== seq) return;
      setSuggestedCapitals([]);
    });
  }, [observerMode, previewStage, pickedGp, previewMapTiles, tileByCoord, resolveCapitalSupport]);

  const applyCapitalSelection = useCallback((preview: CapitalSitePreview) => {
    const seq = pickedCapitalSeqRef.current + 1;
    pickedCapitalSeqRef.current = seq;
    hoveredCapitalSeqRef.current = seq;
    setPickedCapital(preview);
    setHoveredCapital(preview);
    void resolveCapitalSupport(preview).then(resolved => {
      if (pickedCapitalSeqRef.current !== seq) return;
      setPickedCapital(current => (
        current && current.capital.q === preview.capital.q && current.capital.r === preview.capital.r
          ? resolved
          : current
      ));
      setHoveredCapital(current => (
        current && current.capital.q === preview.capital.q && current.capital.r === preview.capital.r
          ? resolved
          : current
      ));
      setSidebarHoveredCapital(current => (
        current && current.capital.q === preview.capital.q && current.capital.r === preview.capital.r
          ? { ...current, support: resolved.support }
          : current
      ));
    }).catch(() => {});
  }, [resolveCapitalSupport]);

  const handleNationPick = useCallback((idx: number) => {
    hoveredCapitalSeqRef.current += 1;
    pickedCapitalSeqRef.current += 1;
    suggestionsSeqRef.current += 1;
    setPickedNationIdx(idx);
    setHoveredCapital(null);
    setPickedCapital(null);
    setSidebarHoveredCapital(null);
    setSuggestedCapitals([]);
    setPreviewStage('nation');
  }, []);

  const handleTileHover = useCallback((tile: TileData | null) => {
    if (observerMode || previewStage !== 'capital' || !pickedGp) {
      hoveredCapitalSeqRef.current += 1;
      setHoveredCapital(null);
      return;
    }
    const preview = evaluateCapitalSite(tile, tileByCoord, pickedGp.id);
    if (!preview) {
      hoveredCapitalSeqRef.current += 1;
      setHoveredCapital(null);
      return;
    }
    const seq = hoveredCapitalSeqRef.current + 1;
    hoveredCapitalSeqRef.current = seq;
    setHoveredCapital(preview);
    void resolveCapitalSupport(preview).then(resolved => {
      if (hoveredCapitalSeqRef.current !== seq) return;
      setHoveredCapital(current => (
        current && current.capital.q === preview.capital.q && current.capital.r === preview.capital.r
          ? resolved
          : current
      ));
    }).catch(() => {});
  }, [observerMode, previewStage, pickedGp, tileByCoord, resolveCapitalSupport]);

  const handleTileClick = useCallback((tile: TileData) => {
    if (tile.nation_id == null) return;
    if (observerMode) {
      const gp = previewGps.find(g => g.id === tile.nation_id);
      if (gp) setPickedNationIdx(gp.idx);
      return;
    }
    if (previewStage === 'nation') {
      const gp = previewGps.find(g => g.id === tile.nation_id);
      if (gp) handleNationPick(gp.idx);
      return;
    }
    if (!pickedGp) return;
    const preview = evaluateCapitalSite(tile, tileByCoord, pickedGp.id);
    if (preview) {
      applyCapitalSelection(preview);
    }
  }, [observerMode, previewStage, previewGps, pickedGp, tileByCoord, handleNationPick, applyCapitalSelection]);

  const handleEnterCapitalPlacement = () => {
    if (observerMode || pickedNationIdx == null) return;
    hoveredCapitalSeqRef.current += 1;
    setPreviewStage('capital');
    setPreviewMapMode('terrain');
    setHoveredCapital(null);
    setSidebarHoveredCapital(null);
  };

  const handleLeaveCapitalPlacement = () => {
    hoveredCapitalSeqRef.current += 1;
    setPreviewStage('nation');
    setPreviewMapMode('political');
    setHoveredCapital(null);
    setSidebarHoveredCapital(null);
    setPlacementScale(undefined);
    setPlacementOffset(undefined);
  };

  const handleBegin = async () => {
    const idx = pickedNationIdx ?? 0;
    let gameJson: string;
    let capitalOverride: CapitalOverride | null = null;
    if (observerMode) {
      gameJson = selectedScenario
        ? await newObserverScenarioGame(selectedScenario, difficulty, flavorKey)
        : await newObserverGame(effectiveMapKey, difficulty, mapGenConfig, flavorKey);
      if (idx !== 0) {
        gameJson = await setHumanPlayer(gameJson, idx);
      }
    } else {
      if (!pickedCapital) return;
      capitalOverride = pickedCapital.capital;
      gameJson = selectedScenario
        ? await newScenarioGame(selectedScenario, difficulty, idx, flavorKey, capitalOverride)
        : await newGame(effectiveMapKey, difficulty, idx, mapGenConfig, flavorKey, capitalOverride);
    }
    onStartGame(gameJson, {
      mapKey: effectiveMapKey,
      observerMode,
      scenario: selectedScenario,
      difficulty,
      nationIdx: idx,
      capitalOverride,
      mapGenConfig,
      organicBorders,
      hideHexGrid,
    });
  };

  const canBegin = observerMode || pickedCapital != null;
  const canPlaceCapital = pickedNationIdx != null;

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
                {DIFFICULTIES.map((label, i) => (
                  <div
                    key={label}
                    style={difficulty === i ? { ...s.diffBtn, ...s.diffSelected } : s.diffBtn}
                    onClick={() => setDifficulty(i)}
                  >
                    {label}
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
            <button style={s.secondaryBtn} onClick={onRequestLoadSavedGame}>Load Save</button>
            <div style={{ flex: 1 }} />
            <button style={s.startBtn} onClick={() => buildPreview()}>Preview Map</button>
          </div>
        </div>
      </div>
    );
  }

  const showTerrainControls = previewStage === 'nation' && !selectedScenario;
  const showNationPicker = observerMode || previewStage === 'nation';

  return (
    <div style={s.previewPage}>
      <div style={s.previewHeader}>
        <h1 style={s.headerTitle}>Preview</h1>
        <div style={s.previewSub}>
          {selectedScenario ? `Scenario: ${selectedScenario}` : `Seed: ${effectiveMapKey}`}
          {' \u00b7 '}Names: {flavorKey || effectiveMapKey}
          {' \u00b7 '}{DIFFICULTIES[difficulty]}
          {observerMode ? ' \u00b7 Observer Mode' : ''}
          {!observerMode && previewStage === 'capital' ? ' \u00b7 Place Capital' : ''}
        </div>
      </div>
      <div style={s.previewBody}>
        <div ref={mapWrapRef} style={s.mapWrap}>
          <HexMap
            tiles={previewMapTiles}
            mapMode={previewMapMode}
            diplomacyOverlay={null}
            militaryOverlay={null}
            onMapModeChange={(mode: MapMode) => {
              if (mode === 'terrain' || mode === 'political') setPreviewMapMode(mode);
            }}
            onTileClick={handleTileClick}
            onTileHover={handleTileHover}
            disableFogOfWar={true}
            showHiddenResources={!observerMode}
            highlightedNationId={pickedGp?.id ?? null}
            hoveredPreviewTileKey={hoveredPreviewTileKey}
            placedPreviewTileKey={placedPreviewTileKey}
            hideMapModeControl={!observerMode && previewStage === 'capital'}
            hideCapitalMarkers={!observerMode}
            organicBorders={organicBorders}
            hideHexGrid={hideHexGrid}
            limitedMapModes={true}
            scale={previewStage === 'capital' ? placementScale : undefined}
            offset={previewStage === 'capital' ? placementOffset : undefined}
            onScaleChange={previewStage === 'capital' ? setPlacementScale : undefined}
            onOffsetChange={previewStage === 'capital' ? setPlacementOffset : undefined}
          />
        </div>
        <div style={s.sidebar}>
          {showTerrainControls && (
            <div style={s.sidebarSection}>
              <div style={s.sidebarTitle}>
                <span>Terrain</span>
                <span
                  style={{ marginLeft: 12, fontSize: 11, color: '#daa520', cursor: 'pointer', textDecoration: 'underline', textTransform: 'none', letterSpacing: 0 }}
                  onClick={randomizeTerrain}
                >
                  Randomize
                </span>
                <span
                  style={{ marginLeft: 10, fontSize: 11, color: '#9a9a9a', cursor: 'pointer', textDecoration: 'underline', textTransform: 'none', letterSpacing: 0 }}
                  onClick={() => setTerrainMix(DEFAULT_TERRAIN_MIX)}
                >
                  Reset
                </span>
              </div>
              <div style={s.sidebarHint}>
                Same seed — only the world regenerates as you adjust.
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                <div>
                  <div style={s.sliderLabelRow}>
                    <span>Land amount</span>
                    <span style={s.sliderValue}>{terrainMix.land_amount.toFixed(2)}×</span>
                  </div>
                  <input
                    type="range"
                    min={30}
                    max={250}
                    step={5}
                    value={Math.round(terrainMix.land_amount * 100)}
                    onChange={e => {
                      const v = Number(e.target.value) / 100;
                      setTerrainMix(prev => ({ ...prev, land_amount: v }));
                    }}
                    style={s.slider}
                  />
                </div>
                <div>
                  <div style={s.sliderLabelRow}>
                    <span>Sea ring (cells)</span>
                    <span style={s.sliderValue}>{terrainMix.sea_hard_margin}</span>
                  </div>
                  <input
                    type="range"
                    min={0}
                    max={Math.max(0, terrainMix.sea_falloff_radius - 1)}
                    step={1}
                    value={terrainMix.sea_hard_margin}
                    onChange={e => {
                      const v = Number(e.target.value);
                      setTerrainMix(prev => ({ ...prev, sea_hard_margin: v }));
                    }}
                    style={s.slider}
                  />
                </div>
                <div>
                  <div style={s.sliderLabelRow}>
                    <span>Coastline falloff (cells)</span>
                    <span style={s.sliderValue}>{terrainMix.sea_falloff_radius}</span>
                  </div>
                  <input
                    type="range"
                    min={Math.max(1, terrainMix.sea_hard_margin + 1)}
                    max={20}
                    step={1}
                    value={terrainMix.sea_falloff_radius}
                    onChange={e => {
                      const v = Number(e.target.value);
                      setTerrainMix(prev => ({ ...prev, sea_falloff_radius: v }));
                    }}
                    style={s.slider}
                  />
                </div>
                <div>
                  <div style={s.sliderLabelRow}>
                    <span>River sources</span>
                    <span style={s.sliderValue}>{terrainMix.river_source_percent}%</span>
                  </div>
                  <input
                    type="range"
                    min={0}
                    max={100}
                    step={1}
                    value={terrainMix.river_source_percent}
                    onChange={e => {
                      const v = Number(e.target.value);
                      setTerrainMix(prev => ({ ...prev, river_source_percent: v }));
                    }}
                    style={s.slider}
                  />
                </div>
                <div style={{ borderTop: '1px solid #2a2a3a', marginTop: 4, paddingTop: 6 }} />
                {([
                  ['grassland', 'Grassland'],
                  ['forest', 'Forest'],
                  ['hills', 'Hills'],
                  ['mountain', 'Mountain'],
                  ['desert', 'Desert'],
                  ['swamp', 'Swamp'],
                  ['tundra', 'Tundra'],
                ] as Array<[keyof TerrainMix, string]>).map(([key, label]) => (
                  <div key={key as string}>
                    <div style={s.sliderLabelRow}>
                      <span>{label}</span>
                      <span style={s.sliderValue}>{Math.round((terrainMix[key] as number) * 10) / 10}</span>
                    </div>
                    <input
                      type="range"
                      min={0}
                      max={100}
                      step={1}
                      value={terrainMix[key] as number}
                      onChange={e => {
                        const v = Number(e.target.value);
                        setTerrainMix(prev => ({ ...prev, [key]: v }));
                      }}
                      style={s.slider}
                    />
                  </div>
                ))}
                <div>
                  <div style={s.sliderLabelRow}>
                    <span>Tundra at poles</span>
                    <span style={s.sliderValue}>{Math.round(terrainMix.pole_tundra_strength * 100)}%</span>
                  </div>
                  <input
                    type="range"
                    min={0}
                    max={100}
                    step={1}
                    value={Math.round(terrainMix.pole_tundra_strength * 100)}
                    onChange={e => {
                      const v = Number(e.target.value) / 100;
                      setTerrainMix(prev => ({ ...prev, pole_tundra_strength: v }));
                    }}
                    style={s.slider}
                  />
                </div>
              </div>
            </div>
          )}

          {showNationPicker ? (
            <div style={s.sidebarSection}>
              <div style={s.sidebarTitle}>
                {observerMode ? 'Viewpoint Nation' : 'Choose Your Empire'}
              </div>
              <div style={s.sidebarHint}>
                {observerMode
                  ? 'Pick a nation whose ledger and diplomacy screens to view. You can switch in-game.'
                  : 'Choose your nation first. Then place the capital on a hex inside your country.'}
              </div>
              <div style={s.gpList}>
                {previewGps.map(gp => (
                  <div
                    key={gp.id}
                    style={pickedNationIdx === gp.idx ? { ...s.gpRow, ...s.gpRowSelected } : s.gpRow}
                    onClick={() => handleNationPick(gp.idx)}
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
          ) : (
            <>
              <div style={s.sidebarSection}>
                <div style={s.sidebarTitle}>Place Capital</div>
                <div style={s.sidebarHint}>
                  Hover a hex inside {pickedGp?.name} to preview its opening capital yield. Click a valid hex to place the capital, then begin the campaign.
                </div>
              </div>
              {suggestedCapitals.length > 0 && (
                <div style={s.sidebarSection}>
                  <div style={s.sidebarTitle}>Suggested Placements</div>
                  <div style={s.suggestionList}>
                    {suggestedCapitals.map(entry => {
                      const isSelected = pickedCapital?.capital.q === entry.capital.q
                        && pickedCapital?.capital.r === entry.capital.r;
                      const isHovered = sidebarHoveredCapital?.capital.q === entry.capital.q
                        && sidebarHoveredCapital?.capital.r === entry.capital.r;
                      return (
                        <button
                          key={`${entry.capital.q},${entry.capital.r}`}
                          type="button"
                          style={
                            isSelected
                              ? { ...s.suggestionRow, ...s.suggestionRowSelected }
                              : isHovered
                                ? { ...s.suggestionRow, ...s.suggestionRowHovered }
                                : s.suggestionRow
                          }
                          onMouseEnter={() => setSidebarHoveredCapital(entry)}
                          onMouseLeave={() => setSidebarHoveredCapital(current => (
                            current && current.capital.q === entry.capital.q && current.capital.r === entry.capital.r
                              ? null
                              : current
                          ))}
                          onClick={() => applyCapitalSelection(entry)}
                        >
                          <span style={s.suggestionProvince}>{entry.provinceName}</span>
                          <strong style={s.suggestionValue}>👷 {entry.support ?? '—'}</strong>
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
              <div style={s.sidebarSection}>
                <div style={s.sidebarTitle}>🏭 Capital Yields</div>
                <div style={s.yieldsBody}>
                  <div style={s.supportBox}>
                    <span>👷 Supported workers</span>
                    <strong style={s.supportValue}>{activeCapitalPreview?.support ?? '—'}</strong>
                  </div>
                  {activeCapitalPreview?.resources.length ? (
                    <div style={s.resourceList}>
                      {activeCapitalPreview.resources.map(entry => (
                        <div key={entry.resource} style={s.resourceRow}>
                          <span style={s.resourceLabel}>
                            <span>{resourceEmoji(entry.resource)}</span>
                            <span>{entry.resource}</span>
                          </span>
                          <strong>{entry.amount}</strong>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div style={s.sidebarHint}>Hover a valid hex to preview its opening capital yields.</div>
                  )}
                </div>
              </div>
            </>
          )}
        </div>
      </div>
      <div style={s.previewFooter}>
        <button
          style={s.secondaryBtn}
          onClick={observerMode
            ? () => setStep('config')
            : previewStage === 'capital'
              ? handleLeaveCapitalPlacement
              : () => setStep('config')}
        >
          Back
        </button>
        {previewStage === 'nation' && (
          <>
            <button style={s.secondaryBtn} onClick={handleReroll}>Re-roll</button>
            <button
              style={s.secondaryBtn}
              onClick={handleRerollNames}
              title="Re-roll only the country names and flags. Map layout stays the same."
            >
              Re-roll Names
            </button>
          </>
        )}
        <div style={{ flex: 1 }} />
        {observerMode ? (
          <button
            style={s.startBtn}
            onClick={handleBegin}
          >
            Begin Campaign
          </button>
        ) : previewStage === 'nation' ? (
          <button
            style={canPlaceCapital ? s.startBtn : { ...s.startBtn, ...s.startBtnDisabled }}
            disabled={!canPlaceCapital}
            onClick={handleEnterCapitalPlacement}
          >
            Place Capital
          </button>
        ) : (
          <button
            style={canBegin ? s.startBtn : { ...s.startBtn, ...s.startBtnDisabled }}
            disabled={!canBegin}
            onClick={handleBegin}
          >
            Begin Campaign
          </button>
        )}
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

  previewPage: { fontFamily: 'Georgia, serif', background: '#1a1a2e', color: '#e0d8c0', height: '100vh', display: 'flex', flexDirection: 'column' as const },
  previewHeader: { background: '#0f0f23', padding: '10px 20px', borderBottom: '2px solid #3a3520', textAlign: 'center' as const },
  previewSub: { fontSize: 12, color: '#9a9a9a', marginTop: 2 },
  previewBody: { flex: 1, display: 'flex', overflow: 'hidden' },
  mapWrap: { flex: 1, position: 'relative' as const, overflow: 'hidden' },
  sidebar: { width: 320, background: '#161625', borderLeft: '2px solid #3a3520', padding: 16, overflowY: 'auto' as const, display: 'flex', flexDirection: 'column' as const, gap: 18 },
  sidebarSection: { display: 'flex', flexDirection: 'column' as const, gap: 8 },
  sidebarTitle: { fontSize: 14, color: '#daa520', textTransform: 'uppercase' as const, letterSpacing: 0.5 },
  sidebarHint: { fontSize: 11, color: '#9a9a9a', lineHeight: 1.4 },
  yieldsBody: { display: 'flex', flexDirection: 'column' as const, gap: 8 },
  gpList: { display: 'flex', flexDirection: 'column' as const, gap: 6 },
  gpRow: { display: 'flex', alignItems: 'center', gap: 10, padding: '8px 10px', background: '#1a1a2e', border: '1px solid #3a3520', borderRadius: 3, cursor: 'pointer' },
  gpRowSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.08)' },
  gpSwatch: { width: 12, height: 12, borderRadius: '50%', border: '1px solid rgba(255,255,255,0.2)', flexShrink: 0 },
  gpNameBlock: { display: 'flex', flexDirection: 'column' as const, minWidth: 0, flex: 1 },
  gpName: { fontSize: 13, fontWeight: 'bold' as const },
  gpTitle: { fontSize: 11, color: '#9a9a9a', marginTop: 1 },
  previewFooter: { padding: '12px 20px', background: '#0f0f23', borderTop: '2px solid #3a3520', display: 'flex', gap: 10, alignItems: 'center' },
  resourceList: { display: 'flex', flexDirection: 'column' as const, gap: 6 },
  resourceRow: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 12, padding: '6px 8px', background: '#1a1a2e', border: '1px solid #2c2c3e', borderRadius: 3 },
  resourceLabel: { display: 'inline-flex', alignItems: 'center', gap: 8 },
  supportBox: { padding: '8px 10px', display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 12, border: '1px solid #3a3520', background: '#1a1a2e' },
  supportValue: { color: '#daa520' },
  suggestionList: { display: 'flex', flexDirection: 'column' as const, gap: 6 },
  suggestionRow: { width: '100%', display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 10, padding: '8px 10px', background: '#1a1a2e', border: '1px solid #2c2c3e', borderRadius: 3, color: '#e0d8c0', fontFamily: 'Georgia, serif', fontSize: 12, cursor: 'pointer', textAlign: 'left' as const },
  suggestionRowHovered: { borderColor: '#b7a26b', background: 'rgba(255,235,150,0.08)' },
  suggestionRowSelected: { borderColor: '#daa520', background: 'rgba(218,165,32,0.12)' },
  suggestionProvince: { minWidth: 0, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const },
  suggestionValue: { color: '#daa520', flexShrink: 0 },
};
