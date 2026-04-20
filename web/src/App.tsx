import { useState, useEffect, useCallback, useMemo } from 'react';
import {
  initWasm, processTurn, processTurns, setHumanPlayer,
  newGame, newScenarioGame, newObserverGame, newObserverScenarioGame,
  getMapData, getAvailableTechs, researchTech,
  getDiplomacyOverlay, getMilitaryOverlay,
  getUnitsInProvince, getCivilians, getShips, getValidMoveTargets, getBuildableUnits,
  queueUnitMove, cancelUnitMove, deployCivilian, recallCivilian, engineerBuild,
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
  getBattleArchive,
  getLedgerData,
  getAllGPLedgerData,
} from './wasm';
import type {
  TileData, Headline, MapMode, DiplomacyOverlay, DiplomacyOverlayRelation, MilitaryOverlayEntry,
  ArmyUnitDetail, ProvinceUnits, CiviliansData, CivilianDetail, ShipsData,
  ValidMoveTargets, BuildableUnits, PendingMove,
  TransportData, IndustryData, TradeData, DiplomacyScreenData, ProposalData,
  ArchivedNewspaper, LedgerData, GPLedgerEntry,
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
    if (opts.country !== 'all' && !h.text.includes(opts.country)) return false;
    return true;
  });
}


import HexMap from './components/HexMap';
import GameSetup, { type GameStartParams } from './components/GameSetup';
import UnitPanel from './components/UnitPanel';
import CivilianPanel from './components/CivilianPanel';
import NavalPanel from './components/NavalPanel';
import TransportPanel from './components/TransportPanel';
import IndustryPanel from './components/IndustryPanel';
import DiplomacyPanel from './components/DiplomacyPanel';
import LedgerPanel from './components/LedgerPanel';
import NewspaperScreen from './components/NewspaperScreen';
import TradeScreen from './components/TradeScreen';
import BattleScreen from './components/BattleScreen';
import LegendScreen from './components/LegendScreen';
import ProposalModal from './components/ProposalModal';

function App() {
  const [loading, setLoading] = useState(true);
  const [gameJson, setGameJson] = useState<string>('');
  const [tiles, setTiles] = useState<TileData[]>([]);
  const [gameState, setGameState] = useState<any>(null);
  const [selectedTile, setSelectedTile] = useState<TileData | null>(null);
  const [hoveredTile, setHoveredTile] = useState<TileData | null>(null);
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
  const [newsFilterCategory, setNewsFilterCategory] = useState<string>('all');
  const [newsFilterCountry, setNewsFilterCountry] = useState<string>('all');
  const [mapMode, setMapMode] = useState<MapMode>('terrain');
  const [selectedNation, setSelectedNation] = useState<string>('');
  const [statusMessage, setStatusMessage] = useState<string>('');
  const [diplomacyOverlay, setDiplomacyOverlay] = useState<DiplomacyOverlay | null>(null);
  const [militaryOverlay, setMilitaryOverlay] = useState<MilitaryOverlayEntry[] | null>(null);

  // Newspaper archive state
  const [archiveData, setArchiveData] = useState<ArchivedNewspaper[]>([]);

  // Battle state
  const [currentBattles, setCurrentBattles] = useState<LandBattleData[]>([]);
  const [currentNavalBattles, setCurrentNavalBattles] = useState<NavalBattleData[]>([]);
  const battleArchive = useMemo(
    () => activeScreen === 'battle' && gameJson ? getBattleArchive(gameJson) : [],
    [activeScreen, gameJson],
  );

  // Unit interaction state
  const [provinceUnits, setProvinceUnits] = useState<ProvinceUnits | null>(null);
  const [civilians, setCivilians] = useState<CiviliansData | null>(null);
  const [shipsData, setShipsData] = useState<ShipsData | null>(null);
  const [buildable, setBuildable] = useState<BuildableUnits | null>(null);
  const [selectedUnitIds, setSelectedUnitIds] = useState<number[]>([]);
  const [isMovementMode, setIsMovementMode] = useState(false);
  const [validMoveTargets, setValidMoveTargets] = useState<ValidMoveTargets | null>(null);
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
  const isObserver = gameState?.observer_mode === true;
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

  const showError = useCallback((msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(''), 4000);
  }, []);

  // Helper to update all derived state from a new game JSON.
  // Returns true on success, false on error (state unchanged on failure).
  const applyGameJson = useCallback((json: string): boolean => {
    let state;
    try {
      state = JSON.parse(json);
    } catch (err) {
      console.error('Failed to parse game state JSON:', err);
      showError('Failed to parse game state');
      return false;
    }
    if (state.error) {
      showError(state.error);
      return false;
    }
    setGameJson(json);
    setGameState(state);
    setTiles(getMapData(json, disableFogOfWar));
    setTechs(getAvailableTechs(json));
    const nid = state.human_player_nation;
    setCivilians(getCivilians(json, nid));
    setShipsData(getShips(json, nid));
    setBuildable(getBuildableUnits(json, nid));
    setTransportData(getTransportData(json, nid));
    setIndustryData(getIndustryData(json, nid));
    setTradeData(getTradeData(json, nid));
    setDiplomacyScreenData(getDiplomacyScreenData(json, nid));
    setLedgerData(getLedgerData(json, nid));
    setGpLedgerData(getAllGPLedgerData(json));
    return true;
  }, [showError, disableFogOfWar]);

  // Re-fetch tiles when fog of war toggle changes
  useEffect(() => {
    if (gameJson) {
      setTiles(getMapData(gameJson, disableFogOfWar));
    }
  }, [disableFogOfWar, gameJson]);

  const handleGameStart = (json: string, params: GameStartParams) => {
    if (!applyGameJson(json)) return;
    setGameStartParams(params);
    setGameStarted(true);
    try {
      const state = JSON.parse(json);
      const p = state?.nations?.find((n: any) => n.id === state.human_player_nation);
      if (p) setSelectedNation(p.name);
    } catch {
      // applyGameJson already succeeded, so game state is valid — this parse is for the nation name only
    }
  };

  const handleRestart = useCallback(() => {
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
        ? newObserverScenarioGame(p.scenario, p.difficulty)
        : newObserverGame(p.mapKey, p.difficulty);
      if (idx !== 0) {
        json = setHumanPlayer(json, idx);
      }
    } else {
      json = p.scenario
        ? newScenarioGame(p.scenario, p.difficulty, idx)
        : newGame(p.mapKey, p.difficulty, idx);
    }
    const parsed = JSON.parse(json);
    if (parsed.error) { alert(parsed.error); return; }
    if (!applyGameJson(json)) return;
    setGameStartParams({ ...p, nationIdx: idx });
    setActiveScreen('map');
    setProvinceUnits(null);
    setSelectedUnitIds([]);
    setIsMovementMode(false);
    setValidMoveTargets(null);
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
    setHoveredTile(null);
    setStatusMessage('');
  }, [gameStartParams, gameState, observerGps, applyGameJson]);

  const handleEndTurn = useCallback(() => {
    const result = processTurn(gameJson);
    if (result.error) { alert(result.error); return; }
    const newJson = JSON.stringify(result.game);
    if (!applyGameJson(newJson)) return;
    setHeadlines(result.report?.headlines || []);
    setCurrentBattles(result.report?.battles || []);
    setCurrentNavalBattles(result.report?.naval_battles || []);
    setActiveScreen('newspaper');
    // Check for pending proposals
    const newState = JSON.parse(newJson);
    const nid = newState.human_player_nation;
    const proposals = getPendingProposals(newJson, nid);
    setProposalData(proposals);
    // Clear interaction state
    setProvinceUnits(null);
    setSelectedUnitIds([]);
    setIsMovementMode(false);
    setValidMoveTargets(null);
    setIsDeployMode(false);
    setDeployingCivilian(null);
  }, [gameJson, applyGameJson]);

  const dismissNewspaper = useCallback(() => {
    setActiveScreen('map');
    if (proposalData && proposalData.proposals.length > 0) {
      setShowProposals(true);
    }
  }, [proposalData]);

  const handleSkipTurns = useCallback(() => {
    const n = Math.max(1, Math.min(50, skipN | 0));
    const result = processTurns(gameJson, n);
    if ((result as any).error) { alert((result as any).error); return; }
    const newJson = JSON.stringify(result.game);
    if (!applyGameJson(newJson)) return;
    const allHeadlines = result.reports.flatMap(r => r.headlines);
    const allBattles = result.reports.flatMap(r => r.battles);
    const allNavalBattles = result.reports.flatMap(r => r.naval_battles);
    setHeadlines(allHeadlines);
    setCurrentBattles(allBattles);
    setCurrentNavalBattles(allNavalBattles);
    setProvinceUnits(null);
    setSelectedUnitIds([]);
    setIsMovementMode(false);
    setValidMoveTargets(null);
    setIsDeployMode(false);
    setDeployingCivilian(null);
  }, [gameJson, applyGameJson, skipN]);

  const handleChangeViewpoint = useCallback((nationId: number) => {
    const idx = observerGps.findIndex(g => g.id === nationId);
    if (idx < 0) return;
    const newJson = setHumanPlayer(gameJson, idx);
    const parsed = JSON.parse(newJson);
    if (parsed.error) { alert(parsed.error); return; }
    applyGameJson(newJson);
  }, [gameJson, applyGameJson, observerGps]);

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
        if (isMovementMode) { setIsMovementMode(false); setValidMoveTargets(null); setSelectedUnitIds([]); }
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
  }, [activeScreen, showTech, showProposals, handleEndTurn, dismissNewspaper, isMovementMode, isDeployMode]);

  // Fetch overlay data when map mode or selected nation changes
  useEffect(() => {
    if (!gameJson || !gameState) return;
    if (mapMode === 'diplomatic' || mapMode === 'relationship') {
      const nation = gameState.nations?.find((n: any) => n.name === selectedNation);
      if (nation) {
        setDiplomacyOverlay(getDiplomacyOverlay(gameJson, nation.id));
      } else {
        setDiplomacyOverlay(null);
      }
    } else {
      setDiplomacyOverlay(null);
    }
    if (mapMode === 'military' || mapMode === 'naval') {
      setMilitaryOverlay(getMilitaryOverlay(gameJson));
    } else {
      setMilitaryOverlay(null);
    }
  }, [mapMode, selectedNation, gameJson, gameState]);

  const playerNationId = gameState?.human_player_nation ?? 0;

  const handleTileClick = useCallback((tile: TileData) => {
    // Movement mode: clicking a tile executes the move for all selected units
    if (isMovementMode && selectedUnitIds.length > 0 && tile.province_id != null) {
      // F-002: Validate target is in valid move targets
      const isValidTarget = validMoveTargets && (
        validMoveTargets.friendly.some(t => t.province_id === tile.province_id) ||
        validMoveTargets.hostile.some(t => t.province_id === tile.province_id)
      );
      if (!isValidTarget) return;

      // Move all selected units — prevalidate then apply all-or-nothing
      let currentJson = gameJson;
      const results: string[] = [];
      for (const unitId of selectedUnitIds) {
        const cmd = queueUnitMove(currentJson, playerNationId, unitId, tile.province_id);
        if (cmd.ok && cmd.gameJson) {
          currentJson = cmd.gameJson;
          results.push(cmd.gameJson);
        } else {
          // Rollback: don't apply any partial state
          showError(`Move failed: ${cmd.error}. No units moved.`);
          currentJson = gameJson; // reset to original
          results.length = 0;
          break;
        }
      }
      if (results.length > 0) {
        applyGameJson(currentJson);
        if (provinceUnits) {
          setProvinceUnits(getUnitsInProvince(currentJson, tile.province_id));
        }
      }
      setIsMovementMode(false);
      setValidMoveTargets(null);
      setSelectedUnitIds([]);
      return;
    }

    // Deploy mode: clicking a tile deploys the civilian
    if (isDeployMode && deployingCivilian) {
      // F-004: Only allow clicking highlighted deployable tiles
      const tileKey = `${tile.q},${tile.r}`;
      if (!deployableTiles.has(tileKey)) return; // Ignore click on invalid tile, keep mode active

      const cmd = deployCivilian(gameJson, deployingCivilian.id, tile.q, tile.r);
      if (cmd.ok && cmd.gameJson && applyGameJson(cmd.gameJson)) {
        setIsDeployMode(false);
        setDeployingCivilian(null);
        setDeployableTiles(new Set());
      } else if (cmd.error) {
        showError(`Deploy failed: ${cmd.error}`);
      }
      return;
    }

    setSelectedTile(tile);
    if (tile.owner && (mapMode === 'diplomatic' || mapMode === 'relationship')) {
      setSelectedNation(tile.owner);
    }

    // Load province units when clicking a capital tile; clear multi-selection on context switch
    if (tile.is_capital && tile.province_id != null) {
      setProvinceUnits(getUnitsInProvince(gameJson, tile.province_id));
      setSelectedUnitIds([]);
    } else {
      setProvinceUnits(null);
      setSelectedUnitIds([]);
    }
  }, [mapMode, gameJson, playerNationId, isMovementMode, selectedUnitIds, validMoveTargets, isDeployMode, deployingCivilian, deployableTiles, applyGameJson, provinceUnits, showError]);

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

  // Unit interaction handlers
  const handleMoveUnit = useCallback((unitId: number) => {
    const targets = getValidMoveTargets(gameJson, playerNationId, unitId);
    setValidMoveTargets(targets);
    setSelectedUnitIds([unitId]);
    setIsMovementMode(true);
  }, [gameJson, playerNationId]);

  // Multi-unit selection handlers
  const handleToggleUnit = useCallback((unitId: number, shiftKey: boolean) => {
    setSelectedUnitIds(prev => {
      if (shiftKey) {
        // Toggle: add or remove
        return prev.includes(unitId)
          ? prev.filter(id => id !== unitId)
          : [...prev, unitId];
      }
      // Non-shift: single select/deselect
      return prev.includes(unitId) && prev.length === 1 ? [] : [unitId];
    });
  }, []);

  const handleSelectAll = useCallback(() => {
    if (!provinceUnits) return;
    const movableIds = provinceUnits.army_units
      .filter(u => u.category !== 'Garrison' && !pendingMovesDisplay.some(m => m.unit_id === u.id))
      .map(u => u.id);
    setSelectedUnitIds(prev =>
      prev.length === movableIds.length ? [] : movableIds
    );
  }, [provinceUnits, pendingMovesDisplay]);

  const handleMoveSelected = useCallback(() => {
    if (selectedUnitIds.length === 0) return;
    // Compute intersection of valid targets for all selected units
    const allTargets = selectedUnitIds.map(id => getValidMoveTargets(gameJson, playerNationId, id));
    if (allTargets.some(t => !t)) { showError('Could not compute move targets'); return; }

    const firstTargets = allTargets[0]!;
    const friendly = firstTargets.friendly.filter(t =>
      allTargets.every(targets => targets!.friendly.some(f => f.province_id === t.province_id))
    );
    const hostile = firstTargets.hostile.filter(t =>
      allTargets.every(targets => targets!.hostile.some(h => h.province_id === t.province_id))
    );

    setValidMoveTargets({ friendly, hostile });
    setIsMovementMode(true);
  }, [selectedUnitIds, gameJson, playerNationId, showError]);

  const handleCancelMove = useCallback((unitId: number) => {
    const cmd = cancelUnitMove(gameJson, unitId);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Cancel failed: ${cmd.error}`);
  }, [gameJson, applyGameJson, showError]);

  const handleRecruit = useCallback((unitType: string) => {
    if (isObserver) return;
    const cmd = recruitArmyUnit(gameJson, playerNationId, unitType);
    if (cmd.ok && cmd.gameJson && applyGameJson(cmd.gameJson)) {
      if (selectedTile?.province_id != null) {
        setProvinceUnits(getUnitsInProvince(cmd.gameJson, selectedTile.province_id));
      }
    } else if (cmd.error) {
      showError(`Recruit failed: ${cmd.error}`);
    }
  }, [gameJson, playerNationId, applyGameJson, selectedTile, showError]);

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

  const handleRecallCivilian = useCallback((civilianId: number) => {
    if (isObserver) return;
    const cmd = recallCivilian(gameJson, civilianId);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Recall failed: ${cmd.error}`);
  }, [gameJson, applyGameJson, showError]);

  const handleEngineerBuild = useCallback((civilianId: number, kind: EngineerBuildKind) => {
    if (isObserver) return;
    const cmd = engineerBuild(gameJson, civilianId, kind);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Engineer build failed: ${cmd.error}`);
  }, [gameJson, applyGameJson, showError]);

  const handleHireCivilian = useCallback((civType: string) => {
    if (isObserver) return;
    const cmd = hireCivilian(gameJson, playerNationId, civType);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Hire failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleBuildShip = useCallback((shipType: string) => {
    if (isObserver) return;
    const cmd = buildShip(gameJson, playerNationId, shipType);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Build failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  // ── New screen handlers ──────────────────────────────────────────

  const handleBuildFreightCar = useCallback(() => {
    if (isObserver) return;
    const cmd = buildFreightCar(gameJson, playerNationId);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Build failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleSetAllocation = useCallback((resource: string, percentage: number) => {
    if (isObserver) return;
    const cmd = setTransportAllocation(gameJson, playerNationId, resource, percentage);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Allocation failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleExpandBuilding = useCallback((buildingType: string) => {
    if (isObserver) return;
    const cmd = expandBuilding(gameJson, playerNationId, buildingType);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Expand failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleSetSubsidy = useCallback((targetNationId: number, amount: number) => {
    if (isObserver) return;
    const cmd = setTradeSubsidy(gameJson, playerNationId, targetNationId, amount);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Subsidy failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleSetSellOrder = useCallback((commodityType: string, commodityName: string, quantity: number) => {
    if (isObserver) return;
    const cmd = setPlayerSellOrder(gameJson, playerNationId, commodityType, commodityName, quantity);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Sell order failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleSetBuyOrder = useCallback((resource: string, quantity: number, maxPrice: number) => {
    if (isObserver) return;
    const cmd = setPlayerBuyOrder(gameJson, playerNationId, resource, quantity, maxPrice);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Buy order failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  // Diplomacy screen handlers
  const makeDiploHandler = useCallback((fn: (gj: string, nid: number, tid: number) => any, label: string) =>
    (targetId: number) => {
      if (isObserver) return;
      const cmd = fn(gameJson, playerNationId, targetId);
      if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`${label}: ${cmd.error}`);
    }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleDiploBuildConsulate = useCallback((tid: number) => makeDiploHandler(diplomacyBuildConsulate, 'Consulate')(tid), [makeDiploHandler]);
  const handleDiploBuildEmbassy = useCallback((tid: number) => makeDiploHandler(diplomacyBuildEmbassy, 'Embassy')(tid), [makeDiploHandler]);
  const handleDiploProposeNap = useCallback((tid: number) => makeDiploHandler(diplomacyProposeNap, 'NAP')(tid), [makeDiploHandler]);
  const handleDiploProposeAlliance = useCallback((tid: number) => makeDiploHandler(diplomacyProposeAlliance, 'Alliance')(tid), [makeDiploHandler]);
  const handleDiploDeclareWar = useCallback((tid: number) => makeDiploHandler(diplomacyDeclareWar, 'Declare War')(tid), [makeDiploHandler]);
  const handleDiploProposePeace = useCallback((tid: number) => makeDiploHandler(diplomacyProposePeace, 'Peace')(tid), [makeDiploHandler]);

  const handleDiploSendGrant = useCallback((targetId: number, amount: number) => {
    const cmd = diplomacySendGrant(gameJson, playerNationId, targetId, amount);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Grant failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleDiploBreakTreaty = useCallback((targetId: number, treatyType: string) => {
    if (isObserver) return;
    const cmd = diplomacyBreakTreaty(gameJson, playerNationId, targetId, treatyType);
    if (cmd.ok && cmd.gameJson) applyGameJson(cmd.gameJson);
    else if (cmd.error) showError(`Break treaty failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  // Proposal modal handlers
  const handleAcceptProposal = useCallback((index: number) => {
    if (isObserver) return;
    const cmd = acceptProposal(gameJson, playerNationId, index);
    if (cmd.ok && cmd.gameJson) {
      applyGameJson(cmd.gameJson);
      const updated = getPendingProposals(cmd.gameJson, playerNationId);
      setProposalData(updated);
      if (!updated || updated.proposals.length === 0) setShowProposals(false);
    } else if (cmd.error) showError(`Accept failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);

  const handleRejectProposal = useCallback((index: number) => {
    if (isObserver) return;
    const cmd = rejectProposal(gameJson, playerNationId, index);
    if (cmd.ok && cmd.gameJson) {
      applyGameJson(cmd.gameJson);
      const updated = getPendingProposals(cmd.gameJson, playerNationId);
      setProposalData(updated);
      if (!updated || updated.proposals.length === 0) setShowProposals(false);
    } else if (cmd.error) showError(`Reject failed: ${cmd.error}`);
  }, [gameJson, playerNationId, applyGameJson, showError]);


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

  const handleResearch = (techName: string) => {
    if (isObserver) return;
    const result = researchTech(gameJson, techName);
    try {
      const parsed = JSON.parse(result);
      if (parsed.error) { alert(parsed.error); return; }
    } catch { /* applyGameJson will handle parse errors */ }
    if (!applyGameJson(result)) return;
    setShowTech(false);
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

  return (
    <main style={styles.container}>
      {/* Top bar */}
      <div style={styles.topBar} className="top-bar-responsive">
        <span style={styles.title} className="title-text">
          {isObserver ? `Observing: ${playerName}` : `Empire of ${playerName}`}
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
        <button onClick={() => { setArchiveData(getNewspaperArchive(gameJson)); setActiveScreen('newspaper'); }} style={styles.btn}>History</button>
        {isObserver && (
          <>
            <input
              type="number"
              min={1}
              max={50}
              value={skipN}
              onChange={e => setSkipN(Number(e.target.value))}
              style={styles.skipInput}
              title="Number of turns to skip (each turn is fully processed)"
            />
            <button onClick={handleSkipTurns} style={styles.btn}>Skip</button>
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
            onTileHover={setHoveredTile}
            showHiddenResources={showHiddenResources}
            showAiCivilians={showAiCivilians}
            selectedUnit={null}
            pendingMoves={pendingMoveArrows}
            validMoveTargets={validMoveTargets}
            isMovementMode={isMovementMode}
            isDeployMode={isDeployMode}
            deployableTiles={deployableTiles}
            disableFogOfWar={disableFogOfWar}
            scale={mapScale}
            offset={mapOffset}
            onScaleChange={setMapScale}
            onOffsetChange={setMapOffset}
          />
        </div>

        {/* Full-screen views */}
        {activeScreen === 'ledger' && (
          <LedgerPanel entries={gpLedgerData} onClose={() => setActiveScreen('map')} />
        )}
        {activeScreen === 'newspaper' && (() => {
          const countryOptions: string[] = (gameState?.nations || [])
            .filter((n: any) => !!n.name)
            .map((n: any) => n.name);
          const visible = applyNewsFilters(headlines, {
            showNonActions: showAiNonActions,
            category: newsFilterCategory,
            country: newsFilterCountry,
          });
          const playerNews = visible.filter(h => h.text.includes(playerName));
          const worldNews = visible.filter(h => !h.text.includes(playerName));
          // Ensure archive data is available
          const archive = archiveData.length > 0 ? archiveData : getNewspaperArchive(gameJson);
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
            />
          );
        })()}
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
          <LegendScreen onClose={() => setActiveScreen('map')} />
        )}

        {/* Side panel — context-sensitive, hidden for full-screen views */}
        {!isFullScreen(activeScreen) && (
        <div style={styles.sidePanel} className="side-panel-responsive">
          {activeScreen === 'map' && (
            <>
              <h3 style={styles.panelTitle}>Tile Info</h3>
              {selectedTile && (
                <div style={styles.tileSelected}>
                  <div style={styles.tileLabel}>Selected</div>
                  <div style={styles.tileInfo}>
                    <p><b>{selectedTile.terrain}{selectedTile.resource && (!selectedTile.resource_hidden || showHiddenResources) ? ` — ${selectedTile.resource}` : ''}</b></p>
                    <p>Province: {selectedTile.province || 'None'}</p>
                    <p>Owner: {selectedTile.owner || 'None'}</p>
                    {selectedTile.resource && (!selectedTile.resource_hidden || showHiddenResources) && <p>Level: {selectedTile.improvement_level}/{selectedTile.max_improvement_level}</p>}
                    {selectedTile.is_capital && <p>{'\u2605'} Capital</p>}
                    {selectedTile.has_railroad && <p>Railroad</p>}
                    {selectedTile.has_fort && <p>Fort L{selectedTile.fort_level}</p>}
                  </div>
                </div>
              )}
              {hoveredTile && !(selectedTile && hoveredTile.q === selectedTile.q && hoveredTile.r === selectedTile.r) && (
                <div style={styles.tileHovered}>
                  <div style={styles.tileLabelDim}>Hovering</div>
                  <div style={styles.tileInfo}>
                    <p><b>{hoveredTile.terrain}{hoveredTile.resource && (!hoveredTile.resource_hidden || showHiddenResources) ? ` — ${hoveredTile.resource}` : ''}</b></p>
                    <p>Province: {hoveredTile.province || 'None'}</p>
                    <p>Owner: {hoveredTile.owner || 'None'}</p>
                    {hoveredTile.resource && (!hoveredTile.resource_hidden || showHiddenResources) && <p>Level: {hoveredTile.improvement_level}/{hoveredTile.max_improvement_level}</p>}
                    {hoveredTile.is_capital && <p>{'\u2605'} Capital</p>}
                    {hoveredTile.has_railroad && <p>Railroad</p>}
                    {hoveredTile.has_fort && <p>Fort L{hoveredTile.fort_level}</p>}
                  </div>
                </div>
              )}
              {!selectedTile && !hoveredTile && (
                <p style={styles.hint}>Click to pin, hover to preview</p>
              )}

              {/* Mode-specific hover info */}
              {(mapMode === 'diplomatic' || mapMode === 'relationship') && (() => {
                const activeTile = hoveredTile || selectedTile;
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
                const activeTile = hoveredTile || selectedTile;
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
                    onMoveSelected={handleMoveSelected}
                    onMoveUnit={handleMoveUnit}
                    onCancelMove={handleCancelMove}
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
    </main>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: { display: 'flex', flexDirection: 'column', height: '100vh', fontFamily: "'Georgia', serif", background: '#1a1a2e', color: '#e0d8c0' },
  loading: { display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh', fontSize: 24, color: '#c0a060' },
  topBar: { display: 'flex', alignItems: 'center', gap: 20, padding: '8px 16px', background: '#0f0f23', borderBottom: '2px solid #3a3520', flexShrink: 0 },
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
  viewpointSelect: { padding: '4px 8px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif', cursor: 'pointer' },
  mapKeyChip: { padding: '2px 8px', background: '#1a1a2e', color: '#9a9a9a', border: '1px solid #3a3520', fontFamily: 'monospace', fontSize: 12, cursor: 'pointer', userSelect: 'none' as const },
  modal: { position: 'fixed' as const, inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', justifyContent: 'center', alignItems: 'center', zIndex: 100 },
  modalContent: { background: '#1a1a2e', border: '2px solid #daa520', padding: 24, maxWidth: 500, maxHeight: '80vh', overflowY: 'auto' as const },
  headline: { margin: '6px 0', fontSize: 14 },
  techItem: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '4px 0' },
};

export default App;
