import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  initWasm, processTurn, processTurns, setHumanPlayer,
  newGame, newScenarioGame, newObserverGame, newObserverScenarioGame,
  getMapData, getNavyMarkers, getAvailableTechs, researchTech,
  getDiplomacyOverlay, getMilitaryOverlay,
  getUnitsInProvince, getCivilians, getShips, getValidMoveTargets, getBuildableUnits,
  queueUnitMove, cancelUnitMove, disbandUnit, deployCivilian, recallCivilian, engineerBuild,
  type EngineerBuildKind,
  recruitArmyUnit, hireCivilian, buildShip,
  // New screen queries
  getTransportData, buildFreightCar, setTransportAllocation,
  getIndustryData, expandBuilding,
  getTradeData, setTradeSubsidy, setPlayerSellOrder, setPlayerBuyOrder,
  getDiplomacyScreenData,
  diplomacyBuildConsulate, diplomacyBuildEmbassy, diplomacyProposeNap,
  diplomacyProposeAlliance, diplomacyDeclareWar, diplomacySendGrant,
  diplomacyBreakTreaty, diplomacyProposePeace,
  getPendingProposals, acceptProposal, rejectProposal,
  getNewspaperArchive,
  getPoliticalSnapshot,
  getBattleArchive,
  getLedgerData,
  getAllGPLedgerData,
  parseGameJson,
} from './wasm';
import type {
  TileData, NavyMarker, Headline, MapMode, DiplomacyOverlay, DiplomacyOverlayRelation, MilitaryOverlayEntry,
  ArmyUnitDetail, ProvinceUnits, CiviliansData, CivilianDetail, ShipsData,
  ValidMoveTargets, BuildableUnits, PendingMove,
  TransportData, IndustryData, TradeData, DiplomacyScreenData, ProposalData,
  ArchivedNewspaper, PoliticalSnapshot, LedgerData, GPLedgerEntry,
  LandBattleData, NavalBattleData, ArchivedBattleTurn,
} from './wasm';

const CATEGORY_COLORS: Record<string, string> = {
  war:       '#e63946',
  battle:    '#e76f51',
  diplomacy: '#457b9d',
  growth:    '#2a9d8f',
  trade:     '#daa520',
  crisis:    '#9d0208',
  politics:  '#b380e6',
  military:  '#8a9aaf',
  default:   '#e0d8c0',
};

type ScreenTab = 'map' | 'transport' | 'industry' | 'diplomacy' | 'trade' | 'ledger' | 'newspaper' | 'battle' | 'legend';
const SCREEN_TABS: { key: ScreenTab; label: string; hotkey: string }[] = [
  { key: 'map', label: 'Map', hotkey: 'F1' },
  { key: 'transport', label: 'Transport', hotkey: 'F2' },
  { key: 'industry', label: 'Industry', hotkey: 'F3' },
  { key: 'diplomacy', label: 'Diplomacy', hotkey: 'F4' },
  { key: 'trade', label: 'Trade', hotkey: 'F5' },
  { key: 'ledger', label: 'Ledger', hotkey: 'F6' },
  { key: 'newspaper', label: 'News', hotkey: 'F7' },
  { key: 'battle', label: 'Battles', hotkey: 'F8' },
  { key: 'legend', label: 'Legend', hotkey: 'F9' },
];

function isFullScreen(screen: ScreenTab): boolean {
  return ['ledger', 'trade', 'newspaper', 'battle', 'legend'].includes(screen);
}

function extractNationTag(text: string, nations?: any[]): string | null {
  if (!nations) return null;
  for (const n of nations) {
    if (n.nation_type === 'GreatPower' && text.includes(n.name)) return n.name;
  }
  return null;
}


function applyNewsFilters(
  headlines: Headline[],
  opts: { showNonActions: boolean; category: string; country: string },
): Headline[] {
  return headlines.filter(h => {
    if (h.is_non_action && !opts.showNonActions) return false;
    if (opts.category !== 'all' && h.category !== opts.category) return false;
    if (opts.country !== 'all') {
      const nid = parseInt(opts.country, 10);
      if (Number.isNaN(nid)) return true;
      if (!h.nation_ids?.includes(nid)) return false;
    }
    return true;
  });
}


import HexMap, { navyMarkerKey } from './components/HexMap';
import GameSetup, { type GameStartParams } from './components/GameSetup';
import UnitPanel from './components/UnitPanel';
import CivilianPanel from './components/CivilianPanel';
import NavalPanel from './components/NavalPanel';
import TransportPanel from './components/TransportPanel';
import IndustryPanel from './components/IndustryPanel';
import DiplomacyPanel from './components/DiplomacyPanel';
import LedgerPanel from './components/LedgerPanel';
import NewspaperScreen from './components/NewspaperScreen';
import PoliticalMapModal from './components/PoliticalMapModal';
import TradeScreen from './components/TradeScreen';
import BattleScreen from './components/BattleScreen';
import LegendScreen from './components/LegendScreen';
import ProposalModal from './components/ProposalModal';
import BusyOverlay from './components/BusyOverlay';
import Flag from './components/Flag';

function turnToYearQ(turn: number): string {
  const year = 1815 + Math.floor((turn - 1) / 4);
  return `${year} Q${((turn - 1) % 4) + 1}`;
}

function App() {
  const [loading, setLoading] = useState(true);
  const [busyMessage, setBusyMessage] = useState<string | null>(null);
  // Any mutating handler (turn, diplomacy, unit commands, civilian builds, trade/industry/transport
  // settings) acquires this ref to serialize itself against others, preventing overlapping RPCs
  // that would read the same `gameJson` and then race their `applyGameJson` updates.
  const mutationLockRef = useRef(false);
  const [gameJson, setGameJson] = useState<string>('');
  const [tiles, setTiles] = useState<TileData[]>([]);
  const [navyMarkers, setNavyMarkers] = useState<NavyMarker[]>([]);
  // Selection/hover are stored as *keys*, not snapshots, so a stale object
  // cannot linger after the marker list is refreshed at end-of-turn or on a
  // fog/viewpoint change. Derived live markers below (`selectedNavyMarker`,
  // `hoveredNavyMarker`) resolve the key against the current `navyMarkers`.
  const [selectedNavyKey, setSelectedNavyKey] = useState<string | null>(null);
  const [hoveredNavyKey, setHoveredNavyKey] = useState<string | null>(null);
  const selectedNavyMarker = useMemo<NavyMarker | null>(
    () => (selectedNavyKey ? navyMarkers.find(m => navyMarkerKey(m) === selectedNavyKey) ?? null : null),
    [selectedNavyKey, navyMarkers],
  );
  const hoveredNavyMarker = useMemo<NavyMarker | null>(
    () => (hoveredNavyKey ? navyMarkers.find(m => navyMarkerKey(m) === hoveredNavyKey) ?? null : null),
    [hoveredNavyKey, navyMarkers],
  );
  // If a refresh drops the selected key, clear it so the panel doesn't render
  // against a phantom marker.
  useEffect(() => {
    if (selectedNavyKey && !navyMarkers.some(m => navyMarkerKey(m) === selectedNavyKey)) {
      setSelectedNavyKey(null);
    }
  }, [navyMarkers, selectedNavyKey]);
  const [gameState, setGameState] = useState<any>(null);
  const [selectedTile, setSelectedTile] = useState<TileData | null>(null);
  const [headlines, setHeadlines] = useState<Headline[]>([]);
  const [techs, setTechs] = useState<any[]>([]);
  const [showTech, setShowTech] = useState(false);
  const [activeScreen, setActiveScreen] = useState<ScreenTab>('map');
  const [gameStarted, setGameStarted] = useState(false);
  const [showHiddenResources, setShowHiddenResources] = useState(false);
  const [showAiCivilians, setShowAiCivilians] = useState(false);
  const [showAiReasoning, setShowAiReasoning] = useState(false);
  const [showAiNonActions, setShowAiNonActions] = useState(false);
  const [disableFogOfWar, setDisableFogOfWar] = useState(false);
  const [organicBorders, setOrganicBorders] = useState(true);
  const [hideHexGrid, setHideHexGrid] = useState(false);
  const [newsFilterCategory, setNewsFilterCategory] = useState<string>('all');
  const [newsFilterCountry, setNewsFilterCountry] = useState<string>('all');
  const [mapMode, setMapMode] = useState<MapMode>('political');
  const [selectedNation, setSelectedNation] = useState<string>('');
  const [statusMessage, setStatusMessage] = useState<string>('');
  const [diplomacyOverlay, setDiplomacyOverlay] = useState<DiplomacyOverlay | null>(null);
  const [militaryOverlay, setMilitaryOverlay] = useState<MilitaryOverlayEntry[] | null>(null);

  // Newspaper archive state
  const [archiveData, setArchiveData] = useState<ArchivedNewspaper[]>([]);
  const [politicalSnapshot, setPoliticalSnapshot] = useState<PoliticalSnapshot | null>(null);

  // Battle state
  const [currentBattles, setCurrentBattles] = useState<LandBattleData[]>([]);
  const [currentNavalBattles, setCurrentNavalBattles] = useState<NavalBattleData[]>([]);
  const [battleArchive, setBattleArchive] = useState<ArchivedBattleTurn[]>([]);
  useEffect(() => {
    (async () => {
      if (activeScreen === 'battle' && gameJson) {
        setBattleArchive(await getBattleArchive(gameJson));
      } else {
        setBattleArchive([]);
      }
    })();
  }, [activeScreen, gameJson]);

  // Refresh newspaper archive whenever the screen is active and the game advances —
  // the old length-gated version cached the first load forever, so new turns never showed up.
  useEffect(() => {
    if (activeScreen !== 'newspaper' || !gameJson) return;
    let cancelled = false;
    (async () => {
      const archive = await getNewspaperArchive(gameJson);
      if (!cancelled) setArchiveData(archive);
    })();
    return () => { cancelled = true; };
  }, [activeScreen, gameJson]);

  // Unit interaction state
  const [provinceUnits, setProvinceUnits] = useState<ProvinceUnits | null>(null);
  const [civilians, setCivilians] = useState<CiviliansData | null>(null);
  const [shipsData, setShipsData] = useState<ShipsData | null>(null);
  const [buildable, setBuildable] = useState<BuildableUnits | null>(null);
  const [selectedUnitIds, setSelectedUnitIds] = useState<number[]>([]);
  const [isDeployMode, setIsDeployMode] = useState(false);
  const [deployingCivilian, setDeployingCivilian] = useState<CivilianDetail | null>(null);
  const [deployableTiles, setDeployableTiles] = useState<Set<string>>(new Set());
  const [wasmError, setWasmError] = useState<string | null>(null);

  // New screen state
  const [transportData, setTransportData] = useState<TransportData | null>(null);
  const [industryData, setIndustryData] = useState<IndustryData | null>(null);
  const [tradeData, setTradeData] = useState<TradeData | null>(null);
  const [diplomacyScreenData, setDiplomacyScreenData] = useState<DiplomacyScreenData | null>(null);
  const [ledgerData, setLedgerData] = useState<LedgerData | null>(null);
  const [gpLedgerData, setGpLedgerData] = useState<GPLedgerEntry[]>([]);
  // Previous-turn snapshot of the ledger data, kept so the UI can render
  // turn-over-turn deltas on every stat column. Rotated only when the turn
  // number actually advances — not on every refetch — so mid-turn refreshes
  // don't collapse the delta to zero.
  const [prevGpLedgerData, setPrevGpLedgerData] = useState<GPLedgerEntry[] | null>(null);
  const prevLedgerTurnRef = useRef<number | null>(null);
  const [proposalData, setProposalData] = useState<ProposalData | null>(null);
  const [showProposals, setShowProposals] = useState(false);

  // Map zoom/pan state — lifted here so it persists across screen switches
  const [mapScale, setMapScale] = useState(0.7);
  const [mapOffset, setMapOffset] = useState({ x: -200, y: -100 });

  // Game-start params captured from GameSetup (used for Restart and header chip)
  const [gameStartParams, setGameStartParams] = useState<GameStartParams | null>(null);
  const [copiedKey, setCopiedKey] = useState(false);

  // Observer mode state
  const [skipN, setSkipN] = useState<number>(5);
  const [skipUntilText, setSkipUntilText] = useState<string>('');
  const [skipUntilRunning, setSkipUntilRunning] = useState<boolean>(false);
  const isObserver = gameState?.observer_mode === true;
  useEffect(() => {
    if (isObserver) {
      setShowHiddenResources(true);
      setShowAiCivilians(true);
      setShowAiReasoning(true);
      setShowAiNonActions(true);
      setDisableFogOfWar(true);
    }
  }, [isObserver]);
  const observerGps: { id: number; name: string; color: string }[] = useMemo(
    () => (gameState?.nations || [])
      .filter((n: any) => n.nation_type === 'GreatPower')
      .map((n: any) => ({ id: n.id, name: n.name, color: n.color })),
    [gameState],
  );

  useEffect(() => {
    (async () => {
      try {
        await initWasm();
        setLoading(false);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        console.error('WASM initialization failed:', msg);
        setWasmError(`Failed to initialize game engine: ${msg}`);
        setLoading(false);
      }
    })();
  }, []);

  // Serializes mutating handlers: if one is in flight, a second click is ignored.
  // Prevents two handlers from reading the same `gameJson` and racing their applies.
  const runMutation = useCallback(async (fn: () => Promise<void>): Promise<void> => {
    if (mutationLockRef.current) return;
    mutationLockRef.current = true;
    try {
      await fn();
    } finally {
      mutationLockRef.current = false;
    }
  }, []);

  const showError = useCallback((msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(''), 4000);
  }, []);

  // Generation counter: every applyGameJson invocation bumps this; derived setState calls
  // that come back from earlier generations after await are discarded, so an overlapping
  // end-turn + diplomacy action cannot let a stale fetch overwrite newer state.
  const applyGenRef = useRef(0);

  // Helper to update all derived state from a new game JSON.
  // Returns true on success, false on error (state unchanged on failure).
  const applyGameJson = useCallback(async (json: string): Promise<boolean> => {
    let state;
    try {
      state = parseGameJson(json);
    } catch (err) {
      console.error('Failed to parse game state JSON:', err);
      showError('Failed to parse game state');
      return false;
    }
    if (state.error) {
      showError(state.error);
      return false;
    }
    const myGen = ++applyGenRef.current;
    const isCurrent = () => applyGenRef.current === myGen;

    // Fetch everything in parallel, then commit atomically if we're still the latest call.
    const nid = state.human_player_nation;
    const [
      mapData, navyData, techsData,
      civiliansData, shipsRes, buildableData,
      transportRes, industryRes, tradeRes,
      diploRes, ledgerRes, gpLedgerRes,
    ] = await Promise.all([
      getMapData(json, disableFogOfWar),
      getNavyMarkers(json, disableFogOfWar),
      getAvailableTechs(json),
      getCivilians(json, nid),
      getShips(json, nid),
      getBuildableUnits(json, nid),
      getTransportData(json, nid),
      getIndustryData(json, nid),
      getTradeData(json, nid),
      getDiplomacyScreenData(json, nid),
      getLedgerData(json, nid),
      getAllGPLedgerData(json),
    ]);
    if (!isCurrent()) return false;
    setGameJson(json);
    setGameState(state);
    setTiles(mapData);
    setNavyMarkers(navyData);
    setTechs(techsData);
    setCivilians(civiliansData);
    setShipsData(shipsRes);
    setBuildable(buildableData);
    setTransportData(transportRes);
    setIndustryData(industryRes);
    setTradeData(tradeRes);
    setDiplomacyScreenData(diploRes);
    setLedgerData(ledgerRes);
    // Rotate the previous-ledger snapshot only when the turn number has
    // actually advanced since the last captured snapshot. This keeps the
    // delta comparison pinned to "last turn" rather than "last refetch".
    const newTurn: number = state?.turn?.[0] ?? state?.turn ?? 1;
    if (prevLedgerTurnRef.current !== null && newTurn !== prevLedgerTurnRef.current) {
      setPrevGpLedgerData(gpLedgerData);
    }
    prevLedgerTurnRef.current = newTurn;
    setGpLedgerData(gpLedgerRes);
    return true;
  }, [showError, disableFogOfWar, gpLedgerData]);

  // Re-fetch tiles when fog of war toggle changes
  useEffect(() => {
    (async () => {
      if (gameJson) {
        setTiles(await getMapData(gameJson, disableFogOfWar));
        setNavyMarkers(await getNavyMarkers(gameJson, disableFogOfWar));
      }
    })();
  }, [disableFogOfWar, gameJson]);

  const handleGameStart = async (json: string, params: GameStartParams) => {
    await runMutation(async () => {
      if (!(await applyGameJson(json))) return;
      setGameStartParams(params);
      setOrganicBorders(params.organicBorders);
      setHideHexGrid(params.hideHexGrid);
      setGameStarted(true);
      try {
        const state = parseGameJson(json);
        const p = state?.nations?.find((n: any) => n.id === state.human_player_nation);
        if (p) setSelectedNation(p.name);
      } catch {
        // applyGameJson already succeeded, so game state is valid — this parse is for the nation name only
      }
    });
  };

  const handleRestart = useCallback(async () => {
    await runMutation(async () => {
      if (!gameStartParams) return;
      if (!confirm('Restart this map from turn 1?')) return;
      const p = gameStartParams;
      // Restart with the nation currently being viewed, not the one picked at game start —
      // the player may have changed viewpoint during observer mode.
      const currentIdx = observerGps.findIndex(g => g.id === gameState?.human_player_nation);
      const idx = currentIdx >= 0 ? currentIdx : p.nationIdx;
      let json: string;
      if (p.observerMode) {
        json = p.scenario
          ? await newObserverScenarioGame(p.scenario, p.difficulty)
          : await newObserverGame(p.mapKey, p.difficulty, p.mapGenConfig);
        if (idx !== 0) {
          json = await setHumanPlayer(json, idx);
        }
      } else {
        json = p.scenario
          ? await newScenarioGame(p.scenario, p.difficulty, idx)
          : await newGame(p.mapKey, p.difficulty, idx, p.mapGenConfig);
      }
      const parsed = parseGameJson(json);
      if (parsed.error) { alert(parsed.error); return; }
      if (!(await applyGameJson(json))) return;
      setGameStartParams({ ...p, nationIdx: idx });
      setActiveScreen('map');
      setProvinceUnits(null);
      setSelectedUnitIds([]);
      setIsDeployMode(false);
      setDeployingCivilian(null);
      setDeployableTiles(new Set());
      setHeadlines([]);
      setCurrentBattles([]);
      setCurrentNavalBattles([]);
      setProposalData(null);
      setShowProposals(false);
      setArchiveData([]);
      setSelectedTile(null);
      setSelectedNavyKey(null);
      setHoveredNavyKey(null);
      setStatusMessage('');
    });
  }, [gameStartParams, gameState, observerGps, applyGameJson, runMutation]);

  const handleEndTurn = useCallback(async () => {
    await runMutation(async () => {
      setBusyMessage('Processing turn…');
      try {
        const result = await processTurn(gameJson);
        if (result.error) { alert(result.error); return; }
        const newJson = JSON.stringify(result.game);
        if (!(await applyGameJson(newJson))) return;
        setHeadlines(result.report?.headlines || []);
        setCurrentBattles(result.report?.battles || []);
        setCurrentNavalBattles(result.report?.naval_battles || []);
        setActiveScreen('newspaper');
        // Check for pending proposals
        const newState = parseGameJson(newJson);
        const nid = newState.human_player_nation;
        const proposals = await getPendingProposals(newJson, nid);
        setProposalData(proposals);
        // Clear interaction state
        setProvinceUnits(null);
        setSelectedUnitIds([]);
        setIsDeployMode(false);
        setDeployingCivilian(null);
      } finally {
        setBusyMessage(null);
      }
    });
  }, [gameJson, applyGameJson, runMutation]);

  const dismissNewspaper = useCallback(() => {
    setActiveScreen('map');
    if (proposalData && proposalData.proposals.length > 0) {
      setShowProposals(true);
    }
  }, [proposalData]);

  const handleSkipTurns = useCallback(async () => {
    await runMutation(async () => {
      const n = Math.max(1, Math.min(500, skipN | 0));
      let currentJson = gameJson;
      let currentTurn: number = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
      const allHeadlines: typeof headlines = [];
      const allBattles: typeof currentBattles = [];
      const allNavalBattles: typeof currentNavalBattles = [];
      try {
        for (let i = 0; i < n; i++) {
          setBusyMessage(`Processing ${turnToYearQ(currentTurn)}…`);
          const result = await processTurns(currentJson, 1);
          if ((result as any).error) { alert((result as any).error); return; }
          currentJson = JSON.stringify(result.game);
          currentTurn = result.game?.turn?.[0] ?? result.game?.turn ?? (currentTurn + 1);
          allHeadlines.push(...result.reports.flatMap((r: any) => r.headlines));
          allBattles.push(...result.reports.flatMap((r: any) => r.battles));
          allNavalBattles.push(...result.reports.flatMap((r: any) => r.naval_battles));
        }
        if (!(await applyGameJson(currentJson))) return;
        setHeadlines(allHeadlines);
        setCurrentBattles(allBattles);
        setCurrentNavalBattles(allNavalBattles);
        setProvinceUnits(null);
        setSelectedUnitIds([]);
        setIsDeployMode(false);
        setDeployingCivilian(null);
      } finally {
        setBusyMessage(null);
      }
    });
  }, [gameJson, gameState, applyGameJson, skipN, runMutation]);

  const handleSkipUntil = useCallback(async () => {
    if (skipUntilRunning || mutationLockRef.current) return;
    mutationLockRef.current = true;
    setSkipUntilRunning(true);
    const startTurn: number = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
    setBusyMessage(`Processing ${turnToYearQ(startTurn)}…`);
    try {
      const needle = skipUntilText.trim().toLowerCase();
      // When looking for a text match, process one turn at a time so we can
      // stop at the exact matched turn rather than overshoot to a batch end.
      // When the needle is blank, batch-50 is fine since the user is asking
      // to advance to end-of-game.
      const MAX_TURNS = 1000;
      const batchSize = needle ? 1 : 50;
      let currentJson = gameJson;
      const allHeadlines: typeof headlines = [];
      const allBattles: typeof currentBattles = [];
      const allNavalBattles: typeof currentNavalBattles = [];
      let matched = false;
      let stoppedEarly = false;
      let processed = 0;

      while (processed < MAX_TURNS) {
        const result = await processTurns(currentJson, batchSize);
        if ((result as any).error) { alert((result as any).error); return; }
        currentJson = JSON.stringify(result.game);
        processed += result.reports.length;
        const currentTurn: number = result.game?.turn?.[0] ?? result.game?.turn ?? (startTurn + processed);
        setBusyMessage(`Processing ${turnToYearQ(currentTurn)}…`);

        for (const r of result.reports) {
          allHeadlines.push(...r.headlines);
          allBattles.push(...r.battles);
          allNavalBattles.push(...r.naval_battles);
          if (needle) {
            for (const h of r.headlines) {
              if (h.text.toLowerCase().includes(needle) ||
                  (h.reason || '').toLowerCase().includes(needle)) {
                matched = true;
                break;
              }
            }
          }
          if (matched) break;
        }
        if (matched) break;
        if (result.stopped_early || result.reports.length === 0) {
          stoppedEarly = true;
          break;
        }
      }

      if (!(await applyGameJson(currentJson))) return;
      setHeadlines(allHeadlines);
      setCurrentBattles(allBattles);
      setCurrentNavalBattles(allNavalBattles);
      setActiveScreen('newspaper');
      setProvinceUnits(null);
      setSelectedUnitIds([]);
      setIsDeployMode(false);
      setDeployingCivilian(null);
      if (!matched && !stoppedEarly) {
        showError(needle
          ? `Skip Until: no match for "${skipUntilText}" after ${processed} turns (cap reached)`
          : `Skip Until: cap of ${MAX_TURNS} turns reached before game ended`);
      }
    } finally {
      setSkipUntilRunning(false);
      setBusyMessage(null);
      mutationLockRef.current = false;
    }
  }, [gameJson, gameState, applyGameJson, skipUntilText, skipUntilRunning, showError]);

  const handleChangeViewpoint = useCallback(async (nationId: number) => {
    await runMutation(async () => {
      const idx = observerGps.findIndex(g => g.id === nationId);
      if (idx < 0) return;
      const newJson = await setHumanPlayer(gameJson, idx);
      const parsed = parseGameJson(newJson);
      if (parsed.error) { alert(parsed.error); return; }
      await applyGameJson(newJson);
    });
  }, [gameJson, applyGameJson, observerGps, runMutation]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        e.preventDefault();
        if (activeScreen === 'newspaper') {
          dismissNewspaper();
        } else if (!showTech && !showProposals) {
          handleEndTurn();
        }
      }
      if (e.code === 'Escape') {
        if (selectedUnitIds.length > 0) { setSelectedUnitIds([]); }
        else if (isDeployMode) { setIsDeployMode(false); setDeployingCivilian(null); setDeployableTiles(new Set()); }
        else if (showProposals) setShowProposals(false);
        else if (activeScreen === 'newspaper') dismissNewspaper();
        else if (showTech) setShowTech(false);
        else if (isFullScreen(activeScreen)) setActiveScreen('map');
      }
      if (e.code === 'F1') { e.preventDefault(); setActiveScreen('map'); }
      if (e.code === 'F2') { e.preventDefault(); setActiveScreen('transport'); }
      if (e.code === 'F3') { e.preventDefault(); setActiveScreen('industry'); }
      if (e.code === 'F4') { e.preventDefault(); setActiveScreen('diplomacy'); }
      if (e.code === 'F5') { e.preventDefault(); setActiveScreen('trade'); }
      if (e.code === 'F6') { e.preventDefault(); setActiveScreen('ledger'); }
      if (e.code === 'F7') { e.preventDefault(); setActiveScreen('newspaper'); }
      if (e.code === 'F8') { e.preventDefault(); setActiveScreen('battle'); }
      if (e.code === 'F9') { e.preventDefault(); setActiveScreen('legend'); }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeScreen, showTech, showProposals, handleEndTurn, dismissNewspaper, selectedUnitIds, isDeployMode]);

  // Fetch overlay data when map mode or selected nation changes
  useEffect(() => {
    (async () => {
      if (!gameJson || !gameState) return;
      if (mapMode === 'diplomatic' || mapMode === 'relationship') {
        const nation = gameState.nations?.find((n: any) => n.name === selectedNation);
        if (nation) {
          setDiplomacyOverlay(await getDiplomacyOverlay(gameJson, nation.id));
        } else {
          setDiplomacyOverlay(null);
        }
      } else {
        setDiplomacyOverlay(null);
      }
      if (mapMode === 'military' || mapMode === 'naval') {
        setMilitaryOverlay(await getMilitaryOverlay(gameJson));
      } else {
        setMilitaryOverlay(null);
      }
    })();
  }, [mapMode, selectedNation, gameJson, gameState]);

  const playerNationId = gameState?.human_player_nation ?? 0;

  // Having a non-empty selection implicitly arms movement mode.
  // Valid move targets are the intersection of each selected unit's legal destinations.
  const [validMoveTargets, setValidMoveTargets] = useState<ValidMoveTargets | null>(null);
  const validMoveTargetsGenRef = useRef(0);
  useEffect(() => {
    const myGen = ++validMoveTargetsGenRef.current;
    (async () => {
      if (selectedUnitIds.length === 0 || !gameJson) {
        if (validMoveTargetsGenRef.current === myGen) setValidMoveTargets(null);
        return;
      }
      const allTargets = await Promise.all(
        selectedUnitIds.map(id => getValidMoveTargets(gameJson, playerNationId, id))
      );
      if (validMoveTargetsGenRef.current !== myGen) return;
      if (allTargets.some(t => !t)) {
        setValidMoveTargets(null);
        return;
      }
      const first = allTargets[0]!;
      const friendly = first.friendly.filter(t =>
        allTargets.every(targets => targets!.friendly.some(f => f.province_id === t.province_id))
      );
      const hostile = first.hostile.filter(t =>
        allTargets.every(targets => targets!.hostile.some(h => h.province_id === t.province_id))
      );
      setValidMoveTargets({ friendly, hostile });
    })();
  }, [selectedUnitIds, gameJson, playerNationId]);
  const isMovementMode = selectedUnitIds.length > 0 && validMoveTargets !== null;

  const handleTileClick = useCallback(async (tile: TileData) => {
    // Implicit movement mode: if units are selected and the clicked tile is a valid target, move them.
    if (validMoveTargets && selectedUnitIds.length > 0 && tile.province_id != null) {
      const isValidTarget = (
        validMoveTargets.friendly.some(t => t.province_id === tile.province_id) ||
        validMoveTargets.hostile.some(t => t.province_id === tile.province_id)
      );
      if (isValidTarget) {
        await runMutation(async () => {
          let currentJson = gameJson;
          let ok = true;
          for (const unitId of selectedUnitIds) {
            const cmd = await queueUnitMove(currentJson, playerNationId, unitId, tile.province_id!);
            if (cmd.ok && cmd.gameJson) {
              currentJson = cmd.gameJson;
            } else {
              showError(`Move failed: ${cmd.error}. No units moved.`);
              currentJson = gameJson;
              ok = false;
              break;
            }
          }
          if (ok) {
            await applyGameJson(currentJson);
            if (provinceUnits) {
              setProvinceUnits(await getUnitsInProvince(currentJson, tile.province_id!));
            }
          }
          setSelectedUnitIds([]);
        });
        return;
      }
      // Invalid target: fall through to normal tile navigation (clears selection below).
    }

    // Deploy mode: clicking a tile deploys the civilian
    if (isDeployMode && deployingCivilian) {
      // F-004: Only allow clicking highlighted deployable tiles
      const tileKey = `${tile.q},${tile.r}`;
      if (!deployableTiles.has(tileKey)) return; // Ignore click on invalid tile, keep mode active

      await runMutation(async () => {
        const cmd = await deployCivilian(gameJson, deployingCivilian.id, tile.q, tile.r);
        if (cmd.ok && cmd.gameJson && (await applyGameJson(cmd.gameJson))) {
          setIsDeployMode(false);
          setDeployingCivilian(null);
          setDeployableTiles(new Set());
        } else if (cmd.error) {
          showError(`Deploy failed: ${cmd.error}`);
        }
      });
      return;
    }

    setSelectedTile(tile);
    setSelectedNavyKey(null);
    if (tile.owner && tile.terrain !== 'Sea' && (mapMode === 'diplomatic' || mapMode === 'relationship')) {
      setSelectedNation(tile.owner);
    }

    // Load province units when clicking a capital tile; clear multi-selection on context switch
    if (tile.is_capital && tile.province_id != null) {
      setProvinceUnits(await getUnitsInProvince(gameJson, tile.province_id));
      setSelectedUnitIds([]);
    } else {
      setProvinceUnits(null);
      setSelectedUnitIds([]);
    }
  }, [mapMode, gameJson, playerNationId, selectedUnitIds, validMoveTargets, isDeployMode, deployingCivilian, deployableTiles, applyGameJson, provinceUnits, showError, runMutation]);

  const handleNavyMarkerClick = useCallback((marker: NavyMarker | null) => {
    if (!marker) {
      setSelectedNavyKey(null);
      return;
    }
    const key = navyMarkerKey(marker);
    setSelectedNavyKey(prev => (prev === key ? null : key));
    setSelectedTile(null);
    setProvinceUnits(null);
    setSelectedUnitIds([]);
  }, []);

  const handleNavyMarkerHover = useCallback((marker: NavyMarker | null) => {
    setHoveredNavyKey(marker ? navyMarkerKey(marker) : null);
  }, []);

  // Compute pending moves for arrows
  const pendingMoveArrows = useMemo(() => {
    if (!gameState?.pending_moves) return [];
    const playerMoves = gameState.pending_moves.filter((m: any) => {
      const nid = typeof m[0] === 'number' ? m[0] : m[0]?.[0] ?? 0;
      return nid === playerNationId;
    });
    return playerMoves.map((m: any) => {
      const unitId = typeof m[1] === 'number' ? m[1] : m[1]?.[0] ?? 0;
      const destId = typeof m[2] === 'number' ? m[2] : m[2]?.[0] ?? 0;
      // Find source province for this unit
      let sourceId = 0;
      for (const n of (gameState?.nations || [])) {
        const unit = n.army?.find((u: any) => {
          const uid = typeof u.id === 'number' ? u.id : u.id?.[0] ?? 0;
          return uid === unitId;
        });
        if (unit) {
          sourceId = typeof unit.position === 'number' ? unit.position : unit.position?.[0] ?? 0;
          break;
        }
      }
      return { unit_id: unitId, source_province_id: sourceId, dest_province_id: destId };
    });
  }, [gameState, playerNationId]);

  // Pending moves for the side panel display
  const pendingMovesDisplay: PendingMove[] = useMemo(() => {
    if (!gameState?.pending_moves) return [];
    return gameState.pending_moves
      .filter((m: any) => {
        const nid = typeof m[0] === 'number' ? m[0] : m[0]?.[0] ?? 0;
        return nid === playerNationId;
      })
      .map((m: any) => {
        const unitId = typeof m[1] === 'number' ? m[1] : m[1]?.[0] ?? 0;
        const destId = typeof m[2] === 'number' ? m[2] : m[2]?.[0] ?? 0;
        const prov = gameState.provinces?.find((p: any) => {
          const pid = typeof p.id === 'number' ? p.id : p.id?.[0] ?? 0;
          return pid === destId;
        });
        return { unit_id: unitId, destination_province_id: destId, destination_name: prov?.name || '?' };
      });
  }, [gameState, playerNationId]);

  // Determine if selected tile is player's
  const isPlayerProvince = selectedTile?.nation_id === playerNationId && playerNationId != null;
  const isPlayerCapital = isPlayerProvince && selectedTile?.is_country_capital === true;

  const handleToggleUnit = useCallback((unitId: number) => {
    setSelectedUnitIds(prev =>
      prev.includes(unitId) ? prev.filter(id => id !== unitId) : [...prev, unitId]
    );
  }, []);

  const handleSelectAll = useCallback(() => {
    if (!provinceUnits) return;
    const selectableIds = provinceUnits.army_units
      .filter(u => u.category !== 'Garrison')
      .map(u => u.id);
    setSelectedUnitIds(prev =>
      prev.length === selectableIds.length ? [] : selectableIds
    );
  }, [provinceUnits]);

  const handleCancelMove = useCallback(async (unitId: number) => {
    await runMutation(async () => {
      const cmd = await cancelUnitMove(gameJson, unitId);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Cancel failed: ${cmd.error}`);
    });
  }, [gameJson, applyGameJson, showError, runMutation]);

  const handleCancelSelectedMoves = useCallback(async () => {
    await runMutation(async () => {
      const cancelable = selectedUnitIds.filter(
        id => pendingMovesDisplay.some(m => m.unit_id === id)
      );
      if (cancelable.length === 0) return;
      let currentJson = gameJson;
      let succeeded = 0;
      let failed = 0;
      for (const unitId of cancelable) {
        const cmd = await cancelUnitMove(currentJson, unitId);
        if (cmd.ok && cmd.gameJson) {
          currentJson = cmd.gameJson;
          succeeded++;
        } else {
          failed++;
        }
      }
      if (succeeded > 0) await applyGameJson(currentJson);
      if (failed > 0) showError(`Canceled ${succeeded} of ${cancelable.length} moves \u2014 ${failed} failed`);
    });
  }, [selectedUnitIds, pendingMovesDisplay, gameJson, applyGameJson, showError, runMutation]);

  const handleDismissSelected = useCallback(async () => {
    if (isObserver || selectedUnitIds.length === 0) return;
    const n = selectedUnitIds.length;
    const ok = window.confirm(`Dismiss ${n} unit${n > 1 ? 's' : ''}? This cannot be undone.`);
    if (!ok) return;
    await runMutation(async () => {
      let currentJson = gameJson;
      let succeeded = 0;
      let failed = 0;
      for (const unitId of selectedUnitIds) {
        const cmd = await disbandUnit(currentJson, unitId);
        if (cmd.ok && cmd.gameJson) {
          currentJson = cmd.gameJson;
          succeeded++;
        } else {
          failed++;
        }
      }
      if (succeeded > 0) {
        await applyGameJson(currentJson);
        setSelectedUnitIds([]);
      }
      if (failed > 0) showError(`Dismissed ${succeeded} of ${n} units \u2014 ${failed} failed`);
    });
  }, [isObserver, selectedUnitIds, gameJson, applyGameJson, showError, runMutation]);

  const handleRecruit = useCallback(async (unitType: string) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await recruitArmyUnit(gameJson, playerNationId, unitType);
      if (cmd.ok && cmd.gameJson && (await applyGameJson(cmd.gameJson))) {
        if (selectedTile?.province_id != null) {
          setProvinceUnits(await getUnitsInProvince(cmd.gameJson, selectedTile.province_id));
        }
      } else if (cmd.error) {
        showError(`Recruit failed: ${cmd.error}`);
      }
    });
  }, [gameJson, playerNationId, applyGameJson, selectedTile, showError, runMutation]);

  const handleDeployCivilian = useCallback((civ: CivilianDetail) => {
    if (isObserver) return;
    setDeployingCivilian(civ);
    setIsDeployMode(true);
    // Compute deployable tiles — tiles owned by player where civilian type can work
    const validTiles = new Set<string>();
    for (const t of tiles) {
      if (t.nation_id !== playerNationId || t.terrain === 'Sea' || t.civilian_on_tile) continue;
      // Approximate CivilianType::can_improve logic from domain
      // F-012: Only use visible resources (not hidden deposits)
      const res = (t.resource && !t.resource_hidden) ? t.resource : null;
      const ter = t.terrain;
      let canWork = false;
      switch (civ.type) {
        case 'Farmer': canWork = res === 'Grain' || res === 'Fruit' || res === 'Cotton'; break;
        case 'Rancher': canWork = res === 'Wool' || res === 'Livestock' || res === 'Horses'; break;
        case 'Forester': canWork = res === 'Timber'; break;
        case 'Miner': canWork = res === 'Coal' || res === 'Iron'; break;
        case 'Driller': canWork = res === 'Oil'; break;
        case 'Prospector': canWork = ter === 'Hills' || ter === 'Mountain' || ter === 'Swamp' || ter === 'Desert' || ter === 'Tundra'; break;
        case 'Engineer': canWork = true; break; // any land tile
      }
      if (canWork) validTiles.add(`${t.q},${t.r}`);
    }
    setDeployableTiles(validTiles);
  }, [tiles, playerNationId]);

  const handleRecallCivilian = useCallback(async (civilianId: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await recallCivilian(gameJson, civilianId);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Recall failed: ${cmd.error}`);
    });
  }, [gameJson, applyGameJson, showError, runMutation]);

  const handleEngineerBuild = useCallback(async (civilianId: number, kind: EngineerBuildKind) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await engineerBuild(gameJson, civilianId, kind);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Engineer build failed: ${cmd.error}`);
    });
  }, [gameJson, applyGameJson, showError, runMutation]);

  const handleHireCivilian = useCallback(async (civType: string) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await hireCivilian(gameJson, playerNationId, civType);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Hire failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleBuildShip = useCallback(async (shipType: string) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await buildShip(gameJson, playerNationId, shipType);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Build failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  // ── New screen handlers ──────────────────────────────────────────

  const handleBuildFreightCar = useCallback(async () => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await buildFreightCar(gameJson, playerNationId);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Build failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetAllocation = useCallback(async (resource: string, percentage: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setTransportAllocation(gameJson, playerNationId, resource, percentage);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Allocation failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleExpandBuilding = useCallback(async (buildingType: string) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await expandBuilding(gameJson, playerNationId, buildingType);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Expand failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetSubsidy = useCallback(async (targetNationId: number, amount: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setTradeSubsidy(gameJson, playerNationId, targetNationId, amount);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Subsidy failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetSellOrder = useCallback(async (commodityType: string, commodityName: string, quantity: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPlayerSellOrder(gameJson, playerNationId, commodityType, commodityName, quantity);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Sell order failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetBuyOrder = useCallback(async (resource: string, quantity: number, maxPrice: number) => {
    await runMutation(async () => {
      const cmd = await setPlayerBuyOrder(gameJson, playerNationId, resource, quantity, maxPrice);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Buy order failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  // Diplomacy screen handlers
  const makeDiploHandler = useCallback((fn: (gj: string, nid: number, tid: number) => Promise<any>, label: string) =>
    async (targetId: number) => {
      await runMutation(async () => {
        const cmd = await fn(gameJson, playerNationId, targetId);
        if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
        else if (cmd.error) showError(`${label}: ${cmd.error}`);
      });
    }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleDiploBuildConsulate = useCallback((tid: number) => makeDiploHandler(diplomacyBuildConsulate, 'Consulate')(tid), [makeDiploHandler]);
  const handleDiploBuildEmbassy = useCallback((tid: number) => makeDiploHandler(diplomacyBuildEmbassy, 'Embassy')(tid), [makeDiploHandler]);
  const handleDiploProposeNap = useCallback((tid: number) => makeDiploHandler(diplomacyProposeNap, 'NAP')(tid), [makeDiploHandler]);
  const handleDiploProposeAlliance = useCallback((tid: number) => makeDiploHandler(diplomacyProposeAlliance, 'Alliance')(tid), [makeDiploHandler]);
  const handleDiploDeclareWar = useCallback((tid: number) => makeDiploHandler(diplomacyDeclareWar, 'Declare War')(tid), [makeDiploHandler]);
  const handleDiploProposePeace = useCallback((tid: number) => makeDiploHandler(diplomacyProposePeace, 'Peace')(tid), [makeDiploHandler]);

  const handleDiploSendGrant = useCallback(async (targetId: number, amount: number) => {
    await runMutation(async () => {
      const cmd = await diplomacySendGrant(gameJson, playerNationId, targetId, amount);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Grant failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleDiploBreakTreaty = useCallback(async (targetId: number, treatyType: string) => {
    await runMutation(async () => {
      const cmd = await diplomacyBreakTreaty(gameJson, playerNationId, targetId, treatyType);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Break treaty failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  // Proposal modal handlers
  const handleAcceptProposal = useCallback(async (index: number) => {
    await runMutation(async () => {
      const cmd = await acceptProposal(gameJson, playerNationId, index);
      if (cmd.ok && cmd.gameJson) {
        await applyGameJson(cmd.gameJson);
        const updated = await getPendingProposals(cmd.gameJson, playerNationId);
        setProposalData(updated);
        if (!updated || updated.proposals.length === 0) setShowProposals(false);
      } else if (cmd.error) showError(`Accept failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleRejectProposal = useCallback(async (index: number) => {
    await runMutation(async () => {
      const cmd = await rejectProposal(gameJson, playerNationId, index);
      if (cmd.ok && cmd.gameJson) {
        await applyGameJson(cmd.gameJson);
        const updated = await getPendingProposals(cmd.gameJson, playerNationId);
        setProposalData(updated);
        if (!updated || updated.proposals.length === 0) setShowProposals(false);
      } else if (cmd.error) showError(`Reject failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);


  // Look up diplomacy info for a given tile's owner
  const getDiploInfoForTile = useCallback((tile: TileData | null): DiplomacyOverlayRelation | null => {
    if (!tile || !tile.owner || !diplomacyOverlay) return null;
    if (tile.owner === diplomacyOverlay.selected_nation) return null; // self
    return diplomacyOverlay.relations.find(r => r.nation_name === tile.owner) || null;
  }, [diplomacyOverlay]);

  // Look up military info for a given tile's owner
  const getMilitaryInfoForTile = useCallback((tile: TileData | null): MilitaryOverlayEntry | null => {
    if (!tile || !tile.owner || !militaryOverlay) return null;
    return militaryOverlay.find(m => m.nation_name === tile.owner) || null;
  }, [militaryOverlay]);

  // Build the mode-specific rows shown inside the hex tooltip. This used to
  // live in the right-hand side panel as a "Hovering" strip; we keep the
  // "Selected" equivalent in the side panel.
  const renderTooltipModeExtras = useCallback((tile: TileData) => {
    if (mapMode === 'diplomatic' || mapMode === 'relationship') {
      const diploInfo = getDiploInfoForTile(tile);
      // In observer mode no nation is truly "yours" — the viewpoint nation
      // is just the one being observed, so suppress the "Your nation" label.
      const isSelf = !isObserver && tile.owner === selectedNation;
      return (
        <div style={{ marginTop: 6, paddingTop: 6, borderTop: '1px solid #3a3520', fontSize: 11 }}>
          <div style={{ color: '#888', marginBottom: 2 }}>
            {mapMode === 'diplomatic' ? 'Diplomatic' : 'Relationship'} view {'\u2014'} {selectedNation}
          </div>
          {isSelf && tile.owner && <div style={{ color: '#ffd900' }}>Your nation</div>}
          {diploInfo && (
            <div>
              <div><b>{diploInfo.nation_name}</b>: {diploInfo.status} (score: {diploInfo.score >= 0 ? '+' : ''}{diploInfo.score})</div>
              {diploInfo.treaties.length > 0 && <div style={{ color: '#999' }}>Treaties: {diploInfo.treaties.join(', ')}</div>}
              {diploInfo.has_embassy && <div style={{ color: '#999' }}>Embassy established</div>}
              {diploInfo.has_consulate && !diploInfo.has_embassy && <div style={{ color: '#999' }}>Consulate established</div>}
            </div>
          )}
        </div>
      );
    }
    if (mapMode === 'military' || mapMode === 'naval') {
      const milInfo = getMilitaryInfoForTile(tile);
      const isMilitary = mapMode === 'military';
      return (
        <div style={{ marginTop: 6, paddingTop: 6, borderTop: '1px solid #3a3520', fontSize: 11 }}>
          <div style={{ color: '#888', marginBottom: 2 }}>
            {isMilitary ? 'Military' : 'Naval'} strength
          </div>
          {isMilitary
            ? (tile.is_capital && tile.army_unit_count > 0 && (
                <div>Army: {tile.army_unit_count} units, {tile.army_firepower.toFixed(1)} FP</div>
              ))
            : (tile.is_country_capital && tile.naval_ship_count > 0 && (
                <div>Navy: {tile.naval_ship_count} warships, {tile.naval_firepower} FP</div>
              ))
          }
          {milInfo && (
            <div style={{ color: '#bbb' }}>
              {isMilitary
                ? <span>{milInfo.nation_name}: {milInfo.army_unit_count} total units, {milInfo.total_army_fp.toFixed(1)} total FP</span>
                : <span>{milInfo.nation_name}: {milInfo.warship_count} warships, {milInfo.total_naval_fp} total FP</span>
              }
            </div>
          )}
        </div>
      );
    }
    return null;
  }, [mapMode, selectedNation, isObserver, getDiploInfoForTile, getMilitaryInfoForTile]);

  const handleResearch = async (techName: string) => {
    await runMutation(async () => {
      if (isObserver) return;
      const result = await researchTech(gameJson, techName);
      try {
        const parsed = parseGameJson(result);
        if (parsed.error) { alert(parsed.error); return; }
      } catch { /* applyGameJson will handle parse errors */ }
      if (!(await applyGameJson(result))) return;
      setShowTech(false);
    });
  };

  if (loading) return <div style={styles.loading}>Loading Imperialism...</div>;
  if (wasmError) return (
    <div style={{color: '#e63946', padding: '2rem', fontFamily: 'Georgia, serif'}}>
      <h2>Initialization Error</h2>
      <p>{wasmError}</p>
      <p>Try refreshing the page. If the problem persists, check that WebAssembly is enabled in your browser.</p>
    </div>
  );
  if (!gameStarted) return <GameSetup onStartGame={handleGameStart} />;

  const player = gameState?.nations?.find((n: any) => n.id === gameState.human_player_nation);
  const turnNumber = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
  const year = 1815 + Math.floor((turnNumber - 1) / 4);
  const quarter = ((turnNumber - 1) % 4) + 1;
  const playerName = player?.name || '?';
  const playerTitle = player?.government_title || playerName;
  const playerFlag = player?.flag_svg || '';
  const governmentTitleByNationId: Record<number, string> = {};
  for (const n of gameState?.nations || []) {
    if (n?.government_title) governmentTitleByNationId[n.id] = n.government_title;
  }

  return (
    <main style={styles.container}>
      {/* Top bar */}
      <div style={styles.topBar} className="top-bar-responsive">
        <span style={styles.titleGroup} className="title-text">
          <Flag svg={playerFlag} width={36} height={24} title={playerTitle} />
          <span style={styles.title}>
            {isObserver ? `Observing: ${playerTitle}` : playerTitle}
          </span>
        </span>
        {gameStartParams?.scenario ? (
          <span
            style={{ ...styles.mapKeyChip, cursor: 'default' }}
            title={`Scenario: ${gameStartParams.scenario}`}
          >
            📖 {gameStartParams.scenario}
          </span>
        ) : gameStartParams?.mapKey ? (
          <span
            style={styles.mapKeyChip}
            title={copiedKey ? 'Copied!' : 'Click to copy map key'}
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(gameStartParams.mapKey);
                setCopiedKey(true);
                setTimeout(() => setCopiedKey(false), 1200);
              } catch {
                /* clipboard blocked — title attribute reflects current state */
              }
            }}
          >
            🗺 {gameStartParams.mapKey}{copiedKey ? ' ✓' : ''}
          </span>
        ) : null}
        <span>Turn {turnNumber} ({year} Q{quarter})</span>
        <span>Treasury: ${player?.treasury != null ? Math.floor(player.treasury / 100) : 0}</span>
        <span>Provinces: {player?.province_ids?.length || 0}</span>
        {isObserver && (
          <select
            value={player?.id ?? ''}
            onChange={e => handleChangeViewpoint(Number(e.target.value))}
            style={styles.viewpointSelect}
            title="Viewpoint nation"
          >
            {observerGps.map(gp => (
              <option key={gp.id} value={gp.id}>{gp.name}</option>
            ))}
          </select>
        )}
        {!isObserver && <button onClick={() => setShowTech(!showTech)} style={styles.btn}>Tech</button>}
        <button onClick={async () => { setArchiveData(await getNewspaperArchive(gameJson)); setActiveScreen('newspaper'); }} style={styles.btn}>History</button>
        {isObserver && (
          <>
            <input
              type="number"
              min={1}
              max={500}
              value={skipN}
              onChange={e => setSkipN(Number(e.target.value))}
              style={styles.skipInput}
              title="Number of turns to skip (each turn is fully processed)"
            />
            <button onClick={handleSkipTurns} style={styles.btn}>Skip</button>
            <input
              type="text"
              value={skipUntilText}
              onChange={e => setSkipUntilText(e.target.value)}
              placeholder="until…"
              style={styles.skipUntilInput}
              title="Skip turns until the game ends, or (if non-empty) until a headline contains this text. Case-insensitive substring match on headline text or AI reason."
              disabled={skipUntilRunning}
            />
            <button
              onClick={handleSkipUntil}
              style={styles.btn}
              disabled={skipUntilRunning}
              title="Skip until text appears in news, or to end of game if blank"
            >
              {skipUntilRunning ? '…' : 'Skip Until'}
            </button>
          </>
        )}
        <button onClick={handleEndTurn} style={styles.endTurnBtn}>End Turn</button>
        {gameStartParams && (
          <button onClick={handleRestart} style={styles.btn} title="Restart this map from turn 1">↻</button>
        )}
      </div>

      {/* Screen tabs */}
      <div style={styles.screenTabs} className="screen-tabs-responsive">
        {SCREEN_TABS.map(tab => (
          <button
            key={tab.key}
            style={activeScreen === tab.key ? { ...styles.screenTab, ...styles.screenTabActive } : styles.screenTab}
            onClick={() => setActiveScreen(tab.key)}
          >
            {tab.label}
            <span style={activeScreen === tab.key ? styles.hotkeyActive : styles.hotkey}>{tab.hotkey}</span>
          </button>
        ))}
      </div>

      {/* Main area */}
      <div style={styles.mainArea} className="main-area-responsive">
        {/* Map — always mounted, hidden behind full-screen views to preserve zoom/pan */}
        <div style={{ ...styles.mapContainer, display: isFullScreen(activeScreen) ? 'none' : undefined }}>
          <HexMap
            tiles={tiles}
            mapMode={mapMode}
            diplomacyOverlay={diplomacyOverlay}
            militaryOverlay={militaryOverlay}
            onMapModeChange={setMapMode}
            onTileClick={handleTileClick}
            showHiddenResources={showHiddenResources}
            showAiCivilians={showAiCivilians}
            selectedUnit={null}
            pendingMoves={pendingMoveArrows}
            validMoveTargets={validMoveTargets}
            isMovementMode={isMovementMode}
            isDeployMode={isDeployMode}
            deployableTiles={deployableTiles}
            disableFogOfWar={disableFogOfWar}
            organicBorders={organicBorders}
            hideHexGrid={hideHexGrid}
            scale={mapScale}
            offset={mapOffset}
            onScaleChange={setMapScale}
            onOffsetChange={setMapOffset}
            navyMarkers={navyMarkers}
            selectedNavyKey={selectedNavyKey}
            onNavyMarkerClick={handleNavyMarkerClick}
            onNavyMarkerHover={handleNavyMarkerHover}
            renderTooltipModeExtras={renderTooltipModeExtras}
            governmentTitleByNationId={governmentTitleByNationId}
          />
        </div>

        {/* Full-screen views */}
        {activeScreen === 'ledger' && (
          <LedgerPanel entries={gpLedgerData} previousEntries={prevGpLedgerData} onClose={() => setActiveScreen('map')} />
        )}
        {activeScreen === 'newspaper' && (() => {
          const countryOptions: { id: number; name: string }[] = (gameState?.nations || [])
            .filter((n: any) => !!n.name)
            .map((n: any) => ({ id: n.id as number, name: n.name as string }));
          const visible = applyNewsFilters(headlines, {
            showNonActions: showAiNonActions,
            category: newsFilterCategory,
            country: newsFilterCountry,
          });
          const playerNews = visible.filter(h => h.text.includes(playerName));
          const worldNews = visible.filter(h => !h.text.includes(playerName));
          // Ensure archive data is available (populated by effect when entering newspaper screen)
          const archive = archiveData;
          return (
            <NewspaperScreen
              playerName={playerName}
              year={year}
              quarter={quarter}
              turnNumber={turnNumber}
              headlines={headlines}
              playerNews={playerNews}
              worldNews={worldNews}
              archiveData={archive}
              nations={gameState?.nations || []}
              countryOptions={countryOptions}
              newsFilterCategory={newsFilterCategory}
              newsFilterCountry={newsFilterCountry}
              showAiReasoning={showAiReasoning}
              showAiNonActions={showAiNonActions}
              onCategoryChange={setNewsFilterCategory}
              onCountryChange={setNewsFilterCountry}
              onDismiss={dismissNewspaper}
              onClose={() => setActiveScreen('map')}
              onShowMap={async (turn) => {
                const snap = await getPoliticalSnapshot(gameJson, turn);
                if (snap) setPoliticalSnapshot(snap);
                else alert(`No political snapshot available for turn ${turn}.`);
              }}
            />
          );
        })()}
        {politicalSnapshot && (
          <PoliticalMapModal
            snapshot={politicalSnapshot}
            onClose={() => setPoliticalSnapshot(null)}
          />
        )}
        {activeScreen === 'trade' && (
          <TradeScreen
            trade={tradeData}
            onSetSubsidy={handleSetSubsidy}
            onSetSellOrder={handleSetSellOrder}
            onSetBuyOrder={handleSetBuyOrder}
            onClose={() => setActiveScreen('map')}
          />
        )}
        {activeScreen === 'battle' && (
          <BattleScreen
            currentBattles={currentBattles}
            currentNavalBattles={currentNavalBattles}
            archiveData={battleArchive}
            tiles={tiles}
            year={year}
            quarter={quarter}
            onClose={() => setActiveScreen('map')}
          />
        )}
        {activeScreen === 'legend' && (
          <LegendScreen
            nations={gameState?.nations || []}
            onClose={() => setActiveScreen('map')}
          />
        )}

        {/* Side panel — context-sensitive, hidden for full-screen views */}
        {!isFullScreen(activeScreen) && (
        <div style={styles.sidePanel} className="side-panel-responsive">
          {activeScreen === 'map' && (
            <>
              {selectedTile && (() => {
                // For diplomatically incorporated provinces, prefer the
                // original minor's identity so the player still sees its
                // flag and full title — gameplay-wise the overlord rules,
                // but the absorbed nation keeps its face in the UI.
                const displayNationId = selectedTile.incorporated_nation_id ?? selectedTile.nation_id;
                const displayNation = gameState?.nations?.find((n: any) => n.id === displayNationId);
                const ownerTitle = displayNation?.government_title || displayNation?.name || selectedTile.owner || '';
                const ownerFlag = displayNation?.flag_svg || '';
                const showResource = selectedTile.resource && (!selectedTile.resource_hidden || showHiddenResources);
                return (
                  <div style={styles.tileInfo}>
                    {(ownerFlag || ownerTitle) && (
                      <div style={styles.tileOwnerRow}>
                        <Flag svg={ownerFlag} width={48} height={32} title={ownerTitle} />
                        {ownerTitle && <span style={styles.tileOwnerName}>{ownerTitle}</span>}
                      </div>
                    )}
                    <p><b>{selectedTile.terrain}{showResource ? ` — ${selectedTile.resource}` : ''}</b></p>
                    {selectedTile.province && <p>Province: {selectedTile.province}</p>}
                  </div>
                );
              })()}
              {!selectedTile && !selectedNavyMarker && !hoveredNavyMarker && (
                <p style={styles.hint}>Click to pin; hover a hex for a tooltip</p>
              )}

              {/* Navy marker composition — selection wins over hover */}
              {(() => {
                const marker = selectedNavyMarker ?? hoveredNavyMarker;
                if (!marker) return null;
                const isSelected = !!selectedNavyMarker;
                const title = marker.kind === 'beachhead'
                  ? `Beachhead \u2192 ${marker.target_province ?? '?'}`
                  : `Fleet \u2014 ${marker.owner_name}`;
                const byType = Object.entries(marker.by_type);
                const byOp = Object.entries(marker.by_operation);
                return (
                  <div style={{
                    fontSize: 13, padding: '8px 0', borderTop: '1px solid #3a3520',
                    marginTop: 6, opacity: isSelected ? 1 : 0.85,
                  }}>
                    <div style={{ fontSize: 11, color: '#888', marginBottom: 4 }}>
                      {isSelected ? 'Selected navy' : 'Hovering navy'}
                    </div>
                    <div style={{ color: marker.kind === 'beachhead' ? '#ff8059' : '#e0d8c0' }}>
                      <b>{title}</b>
                    </div>
                    <div style={{ fontSize: 12, color: '#bbb', marginTop: 4 }}>
                      {marker.ship_count} ships &middot; {marker.total_fp} FP &middot; {marker.total_hull} hull
                    </div>
                    {byType.length > 0 && (
                      <div style={{ fontSize: 12, color: '#bbb', marginTop: 4 }}>
                        {byType.map(([t, n]) => `${n} ${t}`).join(', ')}
                      </div>
                    )}
                    {byOp.length > 0 && (
                      <div style={{ fontSize: 11, color: '#888', marginTop: 2 }}>
                        {byOp.map(([op, n]) => `${n} ${op}`).join(' \u00b7 ')}
                      </div>
                    )}
                  </div>
                );
              })()}

              {/* Mode-specific info for the Selected tile (hover version lives in the hex tooltip) */}
              {(mapMode === 'diplomatic' || mapMode === 'relationship') && (() => {
                const activeTile = selectedTile;
                const diploInfo = getDiploInfoForTile(activeTile);
                const isSelf = activeTile?.owner === selectedNation;
                return (
                  <div style={{ fontSize: 13, padding: '6px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                    <div style={{ fontSize: 11, color: '#888', marginBottom: 4 }}>
                      {mapMode === 'diplomatic' ? 'Diplomatic' : 'Relationship'} view — {selectedNation}
                    </div>
                    {isSelf && activeTile?.owner && <div style={{ color: '#ffd900' }}>Your nation</div>}
                    {diploInfo && (
                      <div>
                        <div><b>{diploInfo.nation_name}</b>: {diploInfo.status} (score: {diploInfo.score >= 0 ? '+' : ''}{diploInfo.score})</div>
                        {diploInfo.treaties.length > 0 && <div style={{ fontSize: 11, color: '#999' }}>Treaties: {diploInfo.treaties.join(', ')}</div>}
                        {diploInfo.has_embassy && <div style={{ fontSize: 11, color: '#999' }}>Embassy established</div>}
                        {diploInfo.has_consulate && !diploInfo.has_embassy && <div style={{ fontSize: 11, color: '#999' }}>Consulate established</div>}
                      </div>
                    )}
                  </div>
                );
              })()}
              {(mapMode === 'military' || mapMode === 'naval') && (() => {
                const activeTile = selectedTile;
                const milInfo = getMilitaryInfoForTile(activeTile);
                return (
                  <div style={{ fontSize: 13, padding: '6px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                    <div style={{ fontSize: 11, color: '#888', marginBottom: 4 }}>
                      {mapMode === 'military' ? 'Military' : 'Naval'} strength
                    </div>
                    {activeTile?.is_capital && activeTile.army_unit_count > 0 && (
                      <div>Army: {activeTile.army_unit_count} units, {activeTile.army_firepower.toFixed(1)} FP</div>
                    )}
                    {activeTile?.is_country_capital && activeTile.naval_ship_count > 0 && (
                      <div>Navy: {activeTile.naval_ship_count} warships, {activeTile.naval_firepower} FP</div>
                    )}
                    {milInfo && (
                      <div style={{ marginTop: 4, fontSize: 12, color: '#bbb' }}>
                        {mapMode === 'military'
                          ? <span>{milInfo.nation_name}: {milInfo.army_unit_count} total units, {milInfo.total_army_fp.toFixed(1)} total FP</span>
                          : <span>{milInfo.nation_name}: {milInfo.warship_count} warships, {milInfo.total_naval_fp} total FP</span>
                        }
                      </div>
                    )}
                  </div>
                );
              })()}

              {/* Map mode legend */}
              {mapMode === 'diplomatic' && (
                <div style={{ fontSize: 11, padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                  <div style={{ color: '#888', marginBottom: 4 }}>Legend</div>
                  {[
                    { color: '#ffd900', label: 'Self' },
                    { color: '#2ecc40', label: 'Alliance' },
                    { color: '#7fdbff', label: 'NAP' },
                    { color: '#ff4136', label: 'At War' },
                    { color: '#aaaaaa', label: 'Neutral' },
                  ].map(item => (
                    <div key={item.label} style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 2 }}>
                      <span style={{ display: 'inline-block', width: 12, height: 12, background: item.color, border: '1px solid rgba(255,255,255,0.2)' }} />
                      <span>{item.label}</span>
                    </div>
                  ))}
                </div>
              )}
              {mapMode === 'relationship' && (
                <div style={{ fontSize: 11, padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                  <div style={{ color: '#888', marginBottom: 4 }}>Relationship Score</div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span>-100</span>
                    <div style={{ flex: 1, height: 12, background: 'linear-gradient(to right, rgb(220,40,40), rgb(160,160,160) 50%, rgb(40,200,40))', borderRadius: 2 }} />
                    <span>+100</span>
                  </div>
                </div>
              )}
              {mapMode === 'military' && (
                <div style={{ fontSize: 11, padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                  <div style={{ color: '#888', marginBottom: 4 }}>Army Strength (vs average)</div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span>Weak</span>
                    <div style={{ flex: 1, height: 12, background: 'linear-gradient(to right, rgb(220,40,40), rgb(200,200,40) 50%, rgb(40,200,40))', borderRadius: 2 }} />
                    <span>Strong</span>
                  </div>
                </div>
              )}
              {mapMode === 'naval' && (
                <div style={{ fontSize: 11, padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                  <div style={{ color: '#888', marginBottom: 4 }}>Naval Strength (vs average)</div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span>Weak</span>
                    <div style={{ flex: 1, height: 12, background: 'linear-gradient(to right, rgb(220,40,40), rgb(200,200,40) 50%, rgb(40,200,40))', borderRadius: 2 }} />
                    <span>Strong</span>
                  </div>
                </div>
              )}

              {/* Status message (errors, confirmations) */}
              {statusMessage && (
                <div style={{ background: 'rgba(200,50,50,0.2)', border: '1px solid rgba(200,50,50,0.5)', borderRadius: 4, padding: 8, marginBottom: 8, fontSize: 12, color: '#f88' }}>
                  {statusMessage}
                </div>
              )}

              {/* Movement/Deploy mode indicator */}
              {isMovementMode && (
                <div style={{ background: 'rgba(255,200,0,0.15)', border: '1px solid rgba(255,200,0,0.4)', borderRadius: 4, padding: 8, marginBottom: 8, fontSize: 12 }}>
                  <b>Movement Mode</b> — {selectedUnitIds.length > 1 ? `moving ${selectedUnitIds.length} units` : 'moving 1 unit'} — click a highlighted province, or press Escape to cancel.
                </div>
              )}
              {isDeployMode && deployingCivilian && (
                <div style={{ background: 'rgba(46,204,64,0.15)', border: '1px solid rgba(46,204,64,0.4)', borderRadius: 4, padding: 8, marginBottom: 8, fontSize: 12 }}>
                  <b>Deploy {deployingCivilian.type}</b> — click a highlighted tile, or press Escape to cancel.
                </div>
              )}

              {/* Unit Panel — shown when a capital with units is selected */}
              {provinceUnits && (
                <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
                  <UnitPanel
                    provinceUnits={provinceUnits}
                    buildableArmy={buildable?.army || []}
                    treasury={buildable?.treasury || 0}
                    arms={buildable?.arms || 0}
                    pendingMoves={pendingMovesDisplay}
                    isPlayerCapital={isPlayerCapital}
                    isPlayerProvince={isPlayerProvince}
                    selectedUnitIds={selectedUnitIds}
                    onToggleUnit={handleToggleUnit}
                    onSelectAll={handleSelectAll}
                    onCancelMove={handleCancelMove}
                    onCancelSelectedMoves={handleCancelSelectedMoves}
                    onDismissSelected={handleDismissSelected}
                    onRecruit={handleRecruit}
                  />
                </div>
              )}

              {/* Civilian Panel — always shown for player */}
              {civilians && isPlayerProvince && (
                <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
                  <CivilianPanel
                    civilians={civilians}
                    buildableCivilians={buildable?.civilians || []}
                    treasury={buildable?.treasury || 0}
                    onDeploy={handleDeployCivilian}
                    onRecall={handleRecallCivilian}
                    onHire={handleHireCivilian}
                    onEngineerBuild={handleEngineerBuild}
                  />
                </div>
              )}

              {/* Naval Panel — shown at country capital */}
              {shipsData && isPlayerCapital && (
                <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
                  <NavalPanel
                    ships={shipsData}
                    buildableShips={buildable?.ships || []}
                    onBuildShip={handleBuildShip}
                  />
                </div>
              )}

              <h3 style={styles.panelTitle}>UI</h3>
              <div style={{ padding: '4px 0', fontSize: '12px', display: 'flex', flexDirection: 'column' as const, gap: 4 }}>
                <label>
                  <input type="checkbox" checked={organicBorders} onChange={e => setOrganicBorders(e.target.checked)} />
                  {' '}Organic borders
                </label>
                <label>
                  <input type="checkbox" checked={hideHexGrid} onChange={e => setHideHexGrid(e.target.checked)} />
                  {' '}Hide hex grid
                </label>
              </div>

              <h3 style={styles.panelTitle}>Debug</h3>
              <div style={{ padding: '4px 0', fontSize: '12px', display: 'flex', flexDirection: 'column' as const, gap: 4 }}>
                <label>
                  <input type="checkbox" checked={showHiddenResources} onChange={e => setShowHiddenResources(e.target.checked)} />
                  {' '}Show hidden resources
                </label>
                <label>
                  <input type="checkbox" checked={showAiCivilians} onChange={e => setShowAiCivilians(e.target.checked)} />
                  {' '}Show AI civilians
                </label>
                <label>
                  <input type="checkbox" checked={showAiReasoning} onChange={e => setShowAiReasoning(e.target.checked)} />
                  {' '}Show AI reasoning
                </label>
                <label>
                  <input type="checkbox" checked={showAiNonActions} onChange={e => setShowAiNonActions(e.target.checked)} />
                  {' '}Show AI non-actions
                </label>
                <label>
                  <input type="checkbox" checked={disableFogOfWar} onChange={e => setDisableFogOfWar(e.target.checked)} />
                  {' '}Disable fog of war
                </label>
              </div>

              <h3 style={styles.panelTitle}>Nations</h3>
              <div style={styles.nationList}>
                {gameState?.nations?.filter((n: any) => n.nation_type === 'GreatPower').map((n: any) => (
                  <div key={n.id} style={styles.nationItem}>
                    <span>{n.name}</span>
                    <span>{n.province_ids?.length || 0} prov</span>
                  </div>
                ))}
              </div>
            </>
          )}
          {activeScreen === 'transport' && (
            transportData ? (
              <TransportPanel
                transport={transportData}
                onBuildCar={handleBuildFreightCar}
                onSetAllocation={handleSetAllocation}
              />
            ) : (
              <p style={styles.hint}>Loading transport data...</p>
            )
          )}
          {activeScreen === 'industry' && (
            industryData ? (
              <IndustryPanel
                industry={industryData}
                onExpand={handleExpandBuilding}
              />
            ) : (
              <p style={styles.hint}>Loading industry data...</p>
            )
          )}
          {activeScreen === 'diplomacy' && (
            diplomacyScreenData ? (
              <DiplomacyPanel
                diplomacy={diplomacyScreenData}
                onBuildConsulate={handleDiploBuildConsulate}
                onBuildEmbassy={handleDiploBuildEmbassy}
                onProposeNap={handleDiploProposeNap}
                onProposeAlliance={handleDiploProposeAlliance}
                onDeclareWar={handleDiploDeclareWar}
                onSendGrant={handleDiploSendGrant}
                onBreakTreaty={handleDiploBreakTreaty}
                onProposePeace={handleDiploProposePeace}
              />
            ) : (
              <p style={styles.hint}>Loading diplomacy data...</p>
            )
          )}
        </div>
        )}
      </div>{/* end mainArea */}

      {/* Global error toast — visible across all screens including full-screen views */}
      {statusMessage && (
        <div style={{
          position: 'fixed', bottom: 20, left: '50%', transform: 'translateX(-50%)',
          background: 'rgba(200,50,50,0.95)', border: '1px solid rgba(255,80,80,0.8)',
          borderRadius: 6, padding: '10px 20px', fontSize: 13, color: '#fff',
          zIndex: 200, maxWidth: 500, textAlign: 'center',
          boxShadow: '0 4px 12px rgba(0,0,0,0.4)',
        }}>
          {statusMessage}
        </div>
      )}

      {/* Tech panel */}
      {showTech && (
        <div style={styles.modal} onClick={() => setShowTech(false)}>
          <div style={styles.modalContent} onClick={e => e.stopPropagation()}>
            <h2>Available Technologies</h2>
            {techs.length === 0 ? <p>No technologies available this year.</p> :
              techs.map((t, i) => (
                <div key={i} style={styles.techItem}>
                  <span>{t.name} (${t.cost})</span>
                  <button onClick={() => handleResearch(t.name)} style={styles.btn}>Research</button>
                </div>
              ))
            }
            <button onClick={() => setShowTech(false)} style={styles.btn}>Close</button>
          </div>
        </div>
      )}

      {/* Proposal Modal — appears after newspaper when there are pending proposals */}
      {showProposals && proposalData && proposalData.proposals.length > 0 && (
        <ProposalModal
          proposals={proposalData.proposals}
          onAccept={handleAcceptProposal}
          onReject={handleRejectProposal}
          onClose={() => setShowProposals(false)}
        />
      )}

      <BusyOverlay busy={busyMessage !== null} message={busyMessage ?? undefined} />
    </main>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: { display: 'flex', flexDirection: 'column', height: '100vh', fontFamily: "'Georgia', serif", background: '#1a1a2e', color: '#e0d8c0' },
  loading: { display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh', fontSize: 24, color: '#c0a060' },
  topBar: { display: 'flex', alignItems: 'center', gap: 20, padding: '8px 16px', background: '#0f0f23', borderBottom: '2px solid #3a3520', flexShrink: 0 },
  titleGroup: { display: 'inline-flex', alignItems: 'center', gap: 10 },
  title: { fontWeight: 'bold', fontSize: 18, color: '#daa520' },
  screenTabs: { display: 'flex', background: '#0f0f23', borderBottom: '2px solid #3a3520', flexShrink: 0 },
  screenTab: { flex: 1, padding: '10px 8px', textAlign: 'center' as const, fontSize: 13, color: '#9a9a9a', background: 'none', border: 'none', cursor: 'pointer', fontFamily: 'Georgia, serif', borderBottom: '3px solid transparent', display: 'flex', flexDirection: 'column' as const, alignItems: 'center' as const },
  screenTabActive: { color: '#daa520', borderBottomColor: '#daa520', background: 'rgba(218,165,32,0.05)' },
  hotkey: { fontSize: 10, color: '#555', display: 'block', marginTop: 2 },
  hotkeyActive: { fontSize: 10, color: '#8a7530', display: 'block', marginTop: 2 },
  mainArea: { display: 'flex', flex: 1, overflow: 'hidden', minHeight: 0 },
  mapContainer: { flex: 1, background: '#0a0a1a', minHeight: 0, position: 'relative' as const },
  sidePanel: { width: 260, padding: 12, background: '#161625', borderLeft: '2px solid #3a3520', overflowY: 'auto' as const, flexShrink: 0 },
  panelTitle: { margin: '12px 0 6px', color: '#daa520', borderBottom: '1px solid #3a3520', paddingBottom: 4 },
  tileInfo: { fontSize: 13 },
  tileOwnerRow: { display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 },
  tileOwnerName: { fontWeight: 'bold', color: '#daa520' },
  tileSelected: { background: 'rgba(218,165,32,0.1)', border: '1px solid rgba(218,165,32,0.3)', borderRadius: 4, padding: 8, marginBottom: 8 },
  tileHovered: { padding: 8, marginBottom: 8, opacity: 0.8 },
  tileLabel: { fontSize: 11, color: '#daa520', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  tileLabelDim: { fontSize: 11, color: '#888', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  hint: { color: '#9a9a9a', fontStyle: 'italic' },
  nationList: { fontSize: 13 },
  nationItem: { display: 'flex', justifyContent: 'space-between', padding: '2px 0' },
  btn: { padding: '4px 12px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', cursor: 'pointer', fontFamily: 'Georgia, serif' },
  endTurnBtn: { padding: '6px 20px', background: '#8b4513', color: '#fff', border: '1px solid #a0522d', cursor: 'pointer', fontWeight: 'bold', fontFamily: 'Georgia, serif' },
  skipInput: { width: 48, padding: '4px 6px', background: '#1a1a2e', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif' },
  skipUntilInput: { width: 110, padding: '4px 6px', background: '#1a1a2e', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif' },
  viewpointSelect: { padding: '4px 8px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif', cursor: 'pointer' },
  mapKeyChip: { padding: '2px 8px', background: '#1a1a2e', color: '#9a9a9a', border: '1px solid #3a3520', fontFamily: 'monospace', fontSize: 12, cursor: 'pointer', userSelect: 'none' as const },
  modal: { position: 'fixed' as const, inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', justifyContent: 'center', alignItems: 'center', zIndex: 100 },
  modalContent: { background: '#1a1a2e', border: '2px solid #daa520', padding: 24, maxWidth: 500, maxHeight: '80vh', overflowY: 'auto' as const },
  headline: { margin: '6px 0', fontSize: 14 },
  techItem: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '4px 0' },
};

export default App;
