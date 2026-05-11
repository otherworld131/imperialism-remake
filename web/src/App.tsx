import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  initWasm, processTurn, processTurns, setHumanPlayer,
  newGame, newScenarioGame, newObserverGame, newObserverScenarioGame,
  getMapData, getNavyMarkers, getSeaZones,
  getDiplomacyOverlay, getMilitaryOverlay,
  getUnitsInProvince, getCivilians, getShips, getValidMoveTargets, getBuildableUnits,
  queueUnitMove, cancelUnitMove, disbandUnit, deployCivilian, recallCivilian, engineerBuild,
  moveFleet, cancelFleetMove,
  type EngineerBuildKind,
  setPendingShips, setAutoTradeWithMinors, setPendingArmyRecruit,
  upgradeUnit, upgradeUnits,
  // New screen queries
  getTransportData, setPendingFreightCars, setTransportAllocation,
  getIndustryData, expandBuilding, setChainTarget, setPendingCivilianHire, setPendingTraining, setPendingImmigration,
  getTradeData, setTradeSubsidy, setPlayerSellOrder, setPlayerBuyOrder,
  getDiplomacyScreenData,
  diplomacyBuildConsulate, diplomacyBuildEmbassy, diplomacyProposeNap,
  diplomacyProposeAlliance, diplomacyDeclareWar, diplomacySendGrant,
  diplomacyBreakTreaty, diplomacyProposePeace,
  getPendingProposals, acceptProposal, rejectProposal,
  getNewspaperArchive, getNewspaperArchiveSince,
  getPoliticalSnapshot,
  getBattleArchive,
  getLedgerData,
  getAllGPLedgerData,
  getTechScreenData, queueTechResearch, cancelTechResearch,
  parseGameJson, DEFAULT_MAP_GEN_CONFIG,
} from './wasm';
import type {
  TileData, NavyMarker, SeaZone, Headline, MapMode, DiplomacyOverlay, DiplomacyOverlayRelation, MilitaryOverlayEntry,
  ProvinceUnits, CiviliansData, CivilianDetail, ShipsData,
  ValidMoveTargets, BuildableUnits, PendingMove,
  TransportData, IndustryData, TradeData, DiplomacyScreenData, ProposalData,
  ArchivedNewspaper, PoliticalSnapshot, LedgerData, GPLedgerEntry,
  LandBattleData, NavalBattleData, ArchivedBattleTurn,
  TechScreenData,
} from './wasm';


type ScreenTab = 'map' | 'transport' | 'industry' | 'diplomacy' | 'trade' | 'tech' | 'ledger' | 'newspaper' | 'battle' | 'legend';
const SCREEN_TABS: { key: ScreenTab; label: string; hotkey: string }[] = [
  { key: 'map', label: 'Map', hotkey: 'F1' },
  { key: 'transport', label: 'Transport', hotkey: 'F2' },
  { key: 'industry', label: 'Industry', hotkey: 'F3' },
  { key: 'diplomacy', label: 'Diplomacy', hotkey: 'F4' },
  { key: 'trade', label: 'Trade', hotkey: 'F5' },
  { key: 'tech', label: 'Tech', hotkey: 'F6' },
  { key: 'ledger', label: 'Ledger', hotkey: 'F7' },
  { key: 'newspaper', label: 'News', hotkey: 'F8' },
  { key: 'battle', label: 'Battles', hotkey: 'F9' },
  { key: 'legend', label: 'Legend', hotkey: 'F10' },
];

function isFullScreen(screen: ScreenTab): boolean {
  return ['ledger', 'trade', 'tech', 'newspaper', 'battle', 'legend', 'industry'].includes(screen);
}


import HexMap, { navyMarkerKey } from './components/HexMap';
import GameSetup, { type GameStartParams } from './components/GameSetup';
import UnitPanel from './components/UnitPanel';
import CivilianPanel from './components/CivilianPanel';
import NavalPanel from './components/NavalPanel';
import TransportPanel from './components/TransportPanel';
import IndustryPanel from './components/IndustryPanel';
import DiplomacyBottomBar, { type QueuedDiplomacyAction } from './components/DiplomacyBottomBar';
import LedgerPanel from './components/LedgerPanel';
import NewspaperScreen from './components/NewspaperScreen';
import PoliticalMapModal from './components/PoliticalMapModal';
import TradeScreen from './components/TradeScreen';
import BattleScreen from './components/BattleScreen';
import LegendScreen from './components/LegendScreen';
import ProposalModal from './components/ProposalModal';
import BusyOverlay from './components/BusyOverlay';
import TechScreen from './components/TechScreen';
import Flag from './components/Flag';
import { resourceLabel } from './resourceEmoji';

function canTargetNationWithAction(
  action: QueuedDiplomacyAction,
  targetNationId: number,
  diplomacy: DiplomacyScreenData | null,
): boolean {
  const rel = diplomacy?.relations.find(r => r.nation_id === targetNationId);
  if (!rel || rel.is_in_anarchy) return false;
  const a = rel.actions;
  switch (action.kind) {
    case 'consulate': return a.can_build_consulate;
    case 'embassy': return a.can_build_embassy;
    case 'nap': return a.can_propose_nap;
    case 'alliance': return a.can_propose_alliance;
    case 'peace': return a.can_propose_peace;
    case 'grant': return a.can_send_grant;
    case 'breakTreaty': return a.can_break_treaty && a.breakable_treaties.includes(action.treatyType);
    case 'war': return a.can_declare_war;
  }
}

function diplomacyInvalidReasonFor(
  action: QueuedDiplomacyAction,
  targetNationId: number | null | undefined,
  playerNationId: number | null,
  diplomacy: DiplomacyScreenData | null,
): string | null {
  if (targetNationId == null) return 'Click on a foreign nation to target this action.';
  if (targetNationId === playerNationId) return 'Cannot target your own nation.';
  const rel = diplomacy?.relations.find(r => r.nation_id === targetNationId);
  if (!rel) return 'Cannot target this nation.';
  const name = rel.nation_name;
  if (rel.is_in_anarchy) return `${name} is in anarchy — diplomacy is unavailable.`;
  switch (action.kind) {
    case 'consulate':
      if (rel.actions.can_build_consulate) return null;
      if (rel.has_consulate || rel.has_embassy) return `${name} already has a consulate.`;
      return `Cannot build consulate with ${name}.`;
    case 'embassy':
      if (rel.actions.can_build_embassy) return null;
      if (rel.has_embassy) return `${name} already has an embassy.`;
      if (!rel.has_consulate) return `Need a consulate with ${name} before opening an embassy.`;
      return `Cannot build embassy with ${name}.`;
    case 'nap':
      if (rel.actions.can_propose_nap) return null;
      if (rel.treaties.includes('NAP') || rel.treaties.includes('Alliance')) return `Already have a NAP / alliance with ${name}.`;
      if (rel.has_pending_nap) return `NAP proposal already pending with ${name}.`;
      if (rel.at_war) return `At war with ${name} — make peace first.`;
      return `Cannot propose NAP to ${name}.`;
    case 'alliance':
      if (rel.actions.can_propose_alliance) return null;
      if (rel.treaties.includes('Alliance')) return `Already allied with ${name}.`;
      if (rel.has_pending_alliance) return `Alliance proposal already pending with ${name}.`;
      if (rel.at_war) return `At war with ${name} — make peace first.`;
      return `Cannot propose alliance to ${name}.`;
    case 'peace':
      if (rel.actions.can_propose_peace) return null;
      if (!rel.at_war) return `Not at war with ${name}.`;
      if (rel.has_pending_peace) return `Peace proposal already pending with ${name}.`;
      return `Cannot propose peace to ${name}.`;
    case 'grant':
      if (rel.actions.can_send_grant) return null;
      return `Cannot send grant to ${name} right now.`;
    case 'breakTreaty':
      if (rel.actions.can_break_treaty && rel.actions.breakable_treaties.includes(action.treatyType)) return null;
      return `No ${action.treatyType} to break with ${name}.`;
    case 'war':
      if (rel.actions.can_declare_war) return null;
      if (rel.at_war) return `Already at war with ${name}.`;
      return `Cannot declare war on ${name}.`;
  }
}

function turnToYearQ(turn: number): string {
  const year = 1815 + Math.floor((turn - 1) / 4);
  return `${year} Q${((turn - 1) % 4) + 1}`;
}

const PROSPECTOR_TERRAIN = new Set(['Hills', 'Mountain', 'Swamp', 'Desert', 'Tundra']);
const WEB_SAVE_FILES_KEY = 'imperialism.web.savefiles.v1';
const WEB_SAVE_META_KEY = 'imperialism.web.savefiles.meta.v1';
const WEB_QUICKSAVE_KEY = 'imperialism.web.quicksave.v1';
const WEB_SAVE_DB_NAME = 'imperialism.web.saves.db.v1';
const WEB_SAVE_DB_VERSION = 1;
const WEB_SAVE_PAYLOAD_STORE = 'save_payloads';

type WebSaveFile = {
  version: 1;
  id: string;
  name: string;
  savedAtIso: string;
  turnNumber: number;
  playerName: string;
};

type LegacyWebQuickSave = {
  version: 1;
  savedAtIso: string;
  turnNumber: number;
  playerName: string;
  gameJson: string;
};

function readWebSaveFiles(): WebSaveFile[] {
  try {
    const raw = localStorage.getItem(WEB_SAVE_META_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((s: unknown) => {
        const save = s as Partial<WebSaveFile>;
        return (
          save
          && save.version === 1
          && typeof save.id === 'string'
          && typeof save.name === 'string'
          && typeof save.savedAtIso === 'string'
          && typeof save.turnNumber === 'number'
          && typeof save.playerName === 'string'
        );
      })
      .sort((a: WebSaveFile, b: WebSaveFile) => b.savedAtIso.localeCompare(a.savedAtIso));
  } catch {
    return [];
  }
}

function writeWebSaveFiles(files: WebSaveFile[]): void {
  localStorage.setItem(WEB_SAVE_META_KEY, JSON.stringify(files));
}

function openSaveDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(WEB_SAVE_DB_NAME, WEB_SAVE_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(WEB_SAVE_PAYLOAD_STORE)) {
        db.createObjectStore(WEB_SAVE_PAYLOAD_STORE);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('Failed to open IndexedDB'));
  });
}

async function putSavePayload(id: string, gameJson: string): Promise<void> {
  const db = await openSaveDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(WEB_SAVE_PAYLOAD_STORE, 'readwrite');
    tx.objectStore(WEB_SAVE_PAYLOAD_STORE).put(gameJson, id);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error('Failed to write save payload'));
    tx.onabort = () => reject(tx.error ?? new Error('Save payload write aborted'));
  });
  db.close();
}

async function getSavePayload(id: string): Promise<string | null> {
  const db = await openSaveDb();
  const payload = await new Promise<string | null>((resolve, reject) => {
    const tx = db.transaction(WEB_SAVE_PAYLOAD_STORE, 'readonly');
    const request = tx.objectStore(WEB_SAVE_PAYLOAD_STORE).get(id);
    request.onsuccess = () => {
      const value = request.result;
      resolve(typeof value === 'string' ? value : null);
    };
    request.onerror = () => reject(request.error ?? new Error('Failed to read save payload'));
  });
  db.close();
  return payload;
}

async function deleteSavePayload(id: string): Promise<void> {
  const db = await openSaveDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(WEB_SAVE_PAYLOAD_STORE, 'readwrite');
    tx.objectStore(WEB_SAVE_PAYLOAD_STORE).delete(id);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error('Failed to delete save payload'));
    tx.onabort = () => reject(tx.error ?? new Error('Save payload delete aborted'));
  });
  db.close();
}

async function migrateLegacyWebSavesIfNeeded(): Promise<void> {
  if (localStorage.getItem(WEB_SAVE_META_KEY)) return;

  const migrated: WebSaveFile[] = [];
  try {
    const oldRaw = localStorage.getItem(WEB_SAVE_FILES_KEY);
    if (oldRaw) {
      const oldParsed = JSON.parse(oldRaw);
      if (Array.isArray(oldParsed)) {
        for (const s of oldParsed) {
          const save = s as Partial<WebSaveFile> & { gameJson?: unknown };
          if (
            save
            && save.version === 1
            && typeof save.id === 'string'
            && typeof save.name === 'string'
            && typeof save.savedAtIso === 'string'
            && typeof save.turnNumber === 'number'
            && typeof save.playerName === 'string'
            && typeof save.gameJson === 'string'
          ) {
            await putSavePayload(save.id, save.gameJson);
            migrated.push({
              version: 1,
              id: save.id,
              name: save.name,
              savedAtIso: save.savedAtIso,
              turnNumber: save.turnNumber,
              playerName: save.playerName,
            });
          }
        }
      }
    }

    if (migrated.length === 0) {
      const legacyRaw = localStorage.getItem(WEB_QUICKSAVE_KEY);
      if (legacyRaw) {
        const legacy = JSON.parse(legacyRaw) as LegacyWebQuickSave;
        if (legacy.version === 1 && typeof legacy.gameJson === 'string') {
          const id = 'migrated-quicksave';
          await putSavePayload(id, legacy.gameJson);
          migrated.push({
            version: 1,
            id,
            name: 'quicksave',
            savedAtIso: legacy.savedAtIso,
            turnNumber: legacy.turnNumber,
            playerName: legacy.playerName,
          });
        }
      }
    }

    if (migrated.length > 0) {
      writeWebSaveFiles(migrated);
    }
  } finally {
    // Remove legacy bulky keys after migration attempt to avoid future
    // localStorage quota hits caused by old inlined payload copies.
    localStorage.removeItem(WEB_SAVE_FILES_KEY);
    localStorage.removeItem(WEB_QUICKSAVE_KEY);
  }
}

function App() {
  const [loading, setLoading] = useState(true);
  const [busyMessage, setBusyMessage] = useState<string | null>(null);
  // Any mutating handler (turn, diplomacy, unit commands, civilian builds, trade/industry/transport
  // settings) acquires this ref to serialize itself against others, preventing overlapping RPCs
  // that would read the same `gameJson` and then race their `applyGameJson` updates.
  const mutationLockRef = useRef(false);
  const skipCancelRef = useRef(false);
  const newsArchiveCacheRef = useRef(new Map<string, ArchivedNewspaper[]>());
  const latestNewsArchiveRef = useRef<ArchivedNewspaper[]>([]);
  const archiveRequestSeqRef = useRef(0);
  const currentGameJsonRef = useRef('');
  const deferredDerivedRefreshRef = useRef(false);
  const [skipCancellable, setSkipCancellable] = useState(false);
  const [gameJson, setGameJson] = useState<string>('');
  useEffect(() => {
    currentGameJsonRef.current = gameJson;
  }, [gameJson]);
  const [tiles, setTiles] = useState<TileData[]>([]);
  const [navyMarkers, setNavyMarkers] = useState<NavyMarker[]>([]);
  const [seaZones, setSeaZones] = useState<SeaZone[]>([]);
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
  // When no fleet is selected, drop any ship selection tied to a previous
  // fleet (card #471). Selection of a player fleet's ships is wired below,
  // after `playerNationId` becomes available.
  useEffect(() => {
    if (!selectedNavyMarker) {
      setSelectedShipIds([]);
    }
  }, [selectedNavyMarker]);
  const [gameState, setGameState] = useState<any>(null);
  const [selectedTile, setSelectedTile] = useState<TileData | null>(null);
  const [headlines, setHeadlines] = useState<Headline[]>([]);
  const [techScreenData, setTechScreenData] = useState<TechScreenData | null>(null);
  const [activeScreen, setActiveScreen] = useState<ScreenTab>('map');
  const [gameStarted, setGameStarted] = useState(false);
  const [showHiddenResources, setShowHiddenResources] = useState(false);
  const [showAiCivilians, setShowAiCivilians] = useState(false);
  const [showAiReasoning, setShowAiReasoning] = useState(false);
  const [showAiNonActions, setShowAiNonActions] = useState(false);
  const [showPersonalities, setShowPersonalities] = useState(false);
  const [disableFogOfWar, setDisableFogOfWar] = useState(false);
  const [showHealDebug, setShowHealDebug] = useState(false);
  const [showRetreatDebug, setShowRetreatDebug] = useState(false);
  const [showBattleFirepower, setShowBattleFirepower] = useState(false);
  const [organicBorders, setOrganicBorders] = useState(true);
  const [hideHexGrid, setHideHexGrid] = useState(false);
  const [showResources, setShowResources] = useState(true);
  const [showTransportNetwork, setShowTransportNetwork] = useState(true);
  const [showArmies, setShowArmies] = useState(true);
  const [uiFontSize, setUiFontSize] = useState(14);
  const [newsFilterCategory, setNewsFilterCategory] = useState<string>('all');
  const [newsFilterCountry, setNewsFilterCountry] = useState<string>('all');
  const [newsFilterText, setNewsFilterText] = useState<string>('');
  const [mapMode, setMapMode] = useState<MapMode>('political');
  const [selectedNation, setSelectedNation] = useState<string>('');
  const [statusMessage, setStatusMessage] = useState<string>('');
  const [savePickerContext, setSavePickerContext] = useState<'ingame' | 'setup' | null>(null);
  const [webSaveFiles, setWebSaveFiles] = useState<WebSaveFile[]>([]);
  const [diplomacyOverlay, setDiplomacyOverlay] = useState<DiplomacyOverlay | null>(null);
  const [militaryOverlay, setMilitaryOverlay] = useState<MilitaryOverlayEntry[] | null>(null);
  const [queuedDiplomacyAction, setQueuedDiplomacyAction] = useState<QueuedDiplomacyAction | null>(null);
  const [hoveredDiploTile, setHoveredDiploTile] = useState<TileData | null>(null);
  // Ref keeps the latest diplo handlers so handleTileClick (declared earlier) can call them
  const diploActionsRef = useRef<((action: QueuedDiplomacyAction, targetId: number) => void) | null>(null);

  // Clear queued diplomacy action and stale hover target on every screen change
  useEffect(() => {
    setHoveredDiploTile(null);
    if (activeScreen !== 'diplomacy') setQueuedDiplomacyAction(null);
  }, [activeScreen]);

  // Newspaper archive state
  const [archiveData, setArchiveData] = useState<ArchivedNewspaper[]>([]);
  const [archiveLoadState, setArchiveLoadState] = useState<'idle' | 'loading' | 'loaded' | 'error'>('idle');
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

  const loadNewspaperArchive = useCallback(async () => {
    if (!gameJson) {
      latestNewsArchiveRef.current = [];
      setArchiveData([]);
      setArchiveLoadState('idle');
      return;
    }
    const requestedJson = gameJson;
    const requestSeq = ++archiveRequestSeqRef.current;
    const cached = newsArchiveCacheRef.current.get(gameJson);
    if (cached) {
      setArchiveData(cached);
      setArchiveLoadState('loaded');
      return;
    }
    setArchiveLoadState('loading');
    try {
      const previousArchive = latestNewsArchiveRef.current;
      const afterTurn = previousArchive.reduce((max, entry) => Math.max(max, entry.turn), 0);
      const delta = afterTurn > 0
        ? await getNewspaperArchiveSince(gameJson, afterTurn)
        : await getNewspaperArchive(gameJson);
      if (archiveRequestSeqRef.current !== requestSeq || requestedJson !== currentGameJsonRef.current) return;
      const archive = afterTurn > 0 ? [...previousArchive, ...delta] : delta;
      latestNewsArchiveRef.current = archive;
      newsArchiveCacheRef.current.set(gameJson, archive);
      setArchiveData(archive);
      setArchiveLoadState('loaded');
    } catch {
      if (archiveRequestSeqRef.current !== requestSeq || requestedJson !== currentGameJsonRef.current) return;
      setArchiveData([]);
      setArchiveLoadState('error');
    }
  }, [gameJson]);

  // Unit interaction state
  const [provinceUnits, setProvinceUnits] = useState<ProvinceUnits | null>(null);
  const [civilians, setCivilians] = useState<CiviliansData | null>(null);
  const [shipsData, setShipsData] = useState<ShipsData | null>(null);
  const [buildable, setBuildable] = useState<BuildableUnits | null>(null);
  const [selectedUnitIds, setSelectedUnitIds] = useState<number[]>([]);
  // Selected warships in the currently selected fleet (card #471).
  const [selectedShipIds, setSelectedShipIds] = useState<number[]>([]);
  const [isDeployMode, setIsDeployMode] = useState(false);
  const [deployingCivilian, setDeployingCivilian] = useState<CivilianDetail | null>(null);
  const [deployableTiles, setDeployableTiles] = useState<Set<string>>(new Set());
  const [prospectedTiles, setProspectedTiles] = useState<Set<string>>(new Set());
  const [selectedCivilianId, setSelectedCivilianId] = useState<number | null>(null);
  const [pendingEngineerDeploy, setPendingEngineerDeploy] = useState<{ civ: CivilianDetail; q: number; r: number } | null>(null);
  const [wasmError, setWasmError] = useState<string | null>(null);

  // New screen state
  const [transportData, setTransportData] = useState<TransportData | null>(null);
  const [industryData, setIndustryData] = useState<IndustryData | null>(null);
  const [tradeData, setTradeData] = useState<TradeData | null>(null);
  const [diplomacyScreenData, setDiplomacyScreenData] = useState<DiplomacyScreenData | null>(null);
  const [_ledgerData, setLedgerData] = useState<LedgerData | null>(null);
  const [gpLedgerData, setGpLedgerData] = useState<GPLedgerEntry[]>([]);
  const gpLedgerDataRef = useRef<GPLedgerEntry[]>([]);
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
  const applyModeUiDefaults = useCallback((observerMode: boolean) => {
    setShowAiReasoning(false);
    setShowAiNonActions(false);
    setShowRetreatDebug(observerMode);
    setShowBattleFirepower(observerMode);
  }, []);
  useEffect(() => {
    if (isObserver) {
      setShowHiddenResources(true);
      setShowAiCivilians(true);
      setShowPersonalities(true);
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

  const refreshWebSaveFiles = useCallback(() => {
    setWebSaveFiles(readWebSaveFiles());
  }, []);

  useEffect(() => {
    void (async () => {
      await migrateLegacyWebSavesIfNeeded();
      refreshWebSaveFiles();
    })();
  }, [refreshWebSaveFiles]);

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
      mapData, navyData, seaZonesData,
      civiliansData, shipsRes, buildableData,
      transportRes, industryRes, tradeRes,
      diploRes, ledgerRes, gpLedgerRes, techRes,
    ] = await Promise.all([
      getMapData(json, disableFogOfWar),
      getNavyMarkers(json, disableFogOfWar),
      getSeaZones(json),
      getCivilians(json, nid),
      getShips(json, nid),
      getBuildableUnits(json, nid),
      getTransportData(json, nid),
      getIndustryData(json, nid),
      getTradeData(json, nid),
      getDiplomacyScreenData(json, nid),
      getLedgerData(json, nid),
      getAllGPLedgerData(json),
      getTechScreenData(json),
    ]);
    if (!isCurrent()) return false;
    currentGameJsonRef.current = json;
    setGameJson(json);
    setGameState(state);
    setTiles(mapData);
    setNavyMarkers(navyData);
    setSeaZones(seaZonesData);
    setCivilians(civiliansData);
    setShipsData(shipsRes);
    setBuildable(buildableData);
    setTransportData(transportRes);
    setIndustryData(industryRes);
    setTradeData(tradeRes);
    setDiplomacyScreenData(diploRes);
    setTechScreenData(techRes);
    setLedgerData(ledgerRes);
    // Rotate the previous-ledger snapshot only when the turn number has
    // actually advanced since the last captured snapshot. This keeps the
    // delta comparison pinned to "last turn" rather than "last refetch".
    const newTurn: number = state?.turn?.[0] ?? state?.turn ?? 1;
    if (prevLedgerTurnRef.current !== null && newTurn !== prevLedgerTurnRef.current) {
      setPrevGpLedgerData(gpLedgerDataRef.current);
    }
    prevLedgerTurnRef.current = newTurn;
    gpLedgerDataRef.current = gpLedgerRes;
    setGpLedgerData(gpLedgerRes);
    deferredDerivedRefreshRef.current = false;
    return true;
  }, [showError, disableFogOfWar]);

  const applyGameJsonLightweight = useCallback((json: string): boolean => {
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
    deferredDerivedRefreshRef.current = true;
    currentGameJsonRef.current = json;
    setGameJson(json);
    setGameState(state);
    return true;
  }, [showError]);

  useEffect(() => {
    if (!deferredDerivedRefreshRef.current || !gameJson || activeScreen === 'newspaper') return;
    void applyGameJson(gameJson);
  }, [activeScreen, gameJson, applyGameJson]);

  // Re-fetch tiles when fog of war toggle changes. Game-state changes refresh
  // map/navy through applyGameJson/applyGameJsonLightweight, so don't duplicate
  // those worker calls here on every gameJson update.
  useEffect(() => {
    (async () => {
      if (gameJson) {
        setTiles(await getMapData(gameJson, disableFogOfWar));
        setNavyMarkers(await getNavyMarkers(gameJson, disableFogOfWar));
      }
    })();
  }, [disableFogOfWar]);

  const handleGameStart = async (json: string, params: GameStartParams) => {
    await runMutation(async () => {
      if (!(await applyGameJson(json))) return;
      applyModeUiDefaults(params.observerMode);
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
      applyModeUiDefaults(p.observerMode);
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
      setNewsFilterText('');
      setProposalData(null);
      setShowProposals(false);
      latestNewsArchiveRef.current = [];
      setArchiveData([]);
      setArchiveLoadState('idle');
      newsArchiveCacheRef.current.clear();
      setSelectedTile(null);
      setSelectedNavyKey(null);
      setHoveredNavyKey(null);
      setStatusMessage('');
    });
  }, [gameStartParams, gameState, observerGps, applyGameJson, runMutation, applyModeUiDefaults]);

  const handleEndTurn = useCallback(async () => {
    await runMutation(async () => {
      setBusyMessage('Processing turn…');
      try {
        const archivedTurn = gameState?.turn?.[0] ?? gameState?.turn ?? 0;
        const result = await processTurn(gameJson);
        if (result.error) { alert(result.error); return; }
        const newJson = JSON.stringify(result.game);
        setActiveScreen('newspaper');
        if (!applyGameJsonLightweight(newJson)) return;
        setHeadlines(result.report?.headlines || []);
        setCurrentBattles(result.report?.battles || []);
        setCurrentNavalBattles(result.report?.naval_battles || []);
        setNewsFilterText('');
        const turnArchive = archivedTurn > 0
          ? [{
              turn: archivedTurn,
              year: result.report?.year ?? 0,
              quarter: result.report?.quarter ?? 0,
              headlines: result.report?.headlines || [],
            }]
          : [];
        latestNewsArchiveRef.current = [...latestNewsArchiveRef.current, ...turnArchive];
        setArchiveData([]);
        setArchiveLoadState('idle');
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
  }, [gameJson, gameState, applyGameJsonLightweight, runMutation]);

  const dismissNewspaper = useCallback(() => {
    setActiveScreen('map');
    setNewsFilterText('');
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
      let lastBattles: typeof currentBattles = [];
      let lastNavalBattles: typeof currentNavalBattles = [];
      skipCancelRef.current = false;
      setSkipCancellable(true);
      try {
        let processed = 0;
        while (processed < n) {
          if (skipCancelRef.current) break;
          setBusyMessage(`Processing ${turnToYearQ(currentTurn)}… (click to stop)`);
          const batchSize = Math.min(50, n - processed);
          const result = await processTurns(currentJson, batchSize);
          if ((result as any).error) { alert((result as any).error); return; }
          currentJson = JSON.stringify(result.game);
          currentTurn = result.game?.turn?.[0] ?? result.game?.turn ?? (currentTurn + 1);
          processed += result.reports.length;
          allHeadlines.push(...result.reports.flatMap((r: any) => r.headlines));
          lastBattles = result.reports.flatMap((r: any) => r.battles);
          lastNavalBattles = result.reports.flatMap((r: any) => r.naval_battles);
          if (result.stopped_early || result.reports.length === 0) break;
        }
        if (!applyGameJsonLightweight(currentJson)) return;
        setHeadlines(allHeadlines);
        setCurrentBattles(lastBattles);
        setCurrentNavalBattles(lastNavalBattles);
        setNewsFilterText('');
        latestNewsArchiveRef.current = [];
        setArchiveData([]);
        setArchiveLoadState('idle');
        setProvinceUnits(null);
        setSelectedUnitIds([]);
        setIsDeployMode(false);
        setDeployingCivilian(null);
      } finally {
        skipCancelRef.current = false;
        setSkipCancellable(false);
        setBusyMessage(null);
      }
    });
  }, [gameJson, gameState, applyGameJsonLightweight, skipN, runMutation]);

  const handleSkipUntil = useCallback(async () => {
    if (skipUntilRunning || mutationLockRef.current) return;
    mutationLockRef.current = true;
    setSkipUntilRunning(true);
    skipCancelRef.current = false;
    setSkipCancellable(true);
    const startTurn: number = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
    setBusyMessage(`Processing ${turnToYearQ(startTurn)}… (click to stop)`);
    try {
      const needle = skipUntilText.trim().toLowerCase();
      // When looking for a text match, process one turn at a time so we can
      // stop at the exact matched turn rather than overshoot to a batch end.
      // When the needle is blank, batch-50 is fine since the user is asking
      // to advance to end-of-game.
      const MAX_TURNS = 1000;
      const batchSize = needle ? 1 : 50;
      let currentJson = gameJson;
      let displayedHeadlines: typeof headlines = [];
      let lastBattlesSkip: typeof currentBattles = [];
      let lastNavalBattlesSkip: typeof currentNavalBattles = [];
      let matched = false;
      let stoppedEarly = false;
      let processed = 0;

      while (processed < MAX_TURNS) {
        if (skipCancelRef.current) { stoppedEarly = true; break; }
        const result = await processTurns(currentJson, batchSize);
        if ((result as any).error) { alert((result as any).error); return; }
        currentJson = JSON.stringify(result.game);
        processed += result.reports.length;
        const currentTurn: number = result.game?.turn?.[0] ?? result.game?.turn ?? (startTurn + processed);
        setBusyMessage(`Processing ${turnToYearQ(currentTurn)}… (click to stop)`);

        for (const r of result.reports) {
          displayedHeadlines = r.headlines;
          lastBattlesSkip = r.battles;
          lastNavalBattlesSkip = r.naval_battles;
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

      if (!applyGameJsonLightweight(currentJson)) return;
      setHeadlines(displayedHeadlines);
      setCurrentBattles(lastBattlesSkip);
      setCurrentNavalBattles(lastNavalBattlesSkip);
      setNewsFilterCategory('all');
      setNewsFilterCountry('all');
      setNewsFilterText(skipUntilText.trim());
      setActiveScreen('newspaper');
      latestNewsArchiveRef.current = [];
      setArchiveData([]);
      setArchiveLoadState('idle');
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
      skipCancelRef.current = false;
      setSkipCancellable(false);
      setSkipUntilRunning(false);
      setBusyMessage(null);
      mutationLockRef.current = false;
    }
  }, [gameJson, gameState, applyGameJsonLightweight, skipUntilText, skipUntilRunning, showError]);

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

  const handleWebSave = useCallback(async () => {
    if (mutationLockRef.current) {
      showError('Please wait for turn/skip processing to finish before saving.');
      return;
    }
    try {
      const snapshotJson = currentGameJsonRef.current || gameJson;
      const snapshotState = parseGameJson(snapshotJson);
      if (snapshotState?.error) {
        showError('Save failed: invalid game state.');
        return;
      }
      const turnNumber = snapshotState?.turn?.[0] ?? snapshotState?.turn ?? 1;
      const player =
        snapshotState?.nations?.find((n: any) => n.id === snapshotState?.human_player_nation) ?? null;
      const year = 1815 + Math.floor((turnNumber - 1) / 4);
      const quarter = ((turnNumber - 1) % 4) + 1;
      const defaultName = `save_${year}_Q${quarter}`;
      const requestedName = prompt('Save name:', defaultName);
      if (requestedName == null) return;
      const name = requestedName.trim();
      if (!name) {
        showError('Save canceled: filename cannot be empty.');
        return;
      }

      const existing = readWebSaveFiles();
      const overwriteTarget = existing.find(s => s.name === name);
      if (
        overwriteTarget
        && !confirm(`A save named "${name}" already exists. Overwrite it?`)
      ) {
        return;
      }

      const record: WebSaveFile = {
        version: 1,
        id:
          overwriteTarget?.id
          ?? `${Date.now()}_${Math.random().toString(36).slice(2, 10)}`,
        name,
        savedAtIso: new Date().toISOString(),
        turnNumber,
        playerName: player?.name ?? 'Unknown',
      };
      await putSavePayload(record.id, snapshotJson);
      const next = [record, ...existing.filter(s => s.id !== record.id)].sort(
        (a, b) => b.savedAtIso.localeCompare(a.savedAtIso),
      );
      writeWebSaveFiles(next);
      refreshWebSaveFiles();
      alert(`Saved "${record.name}" (${record.playerName}, ${year} Q${quarter}).`);
    } catch (err) {
      console.error('Failed to save in browser:', err);
      showError('Save failed: storage limit reached or unavailable.');
    }
  }, [gameJson, refreshWebSaveFiles, showError]);

  const handleOpenWebLoadPicker = useCallback(async () => {
    if (mutationLockRef.current) {
      showError('Please wait for turn/skip processing to finish before loading.');
      return;
    }
    await migrateLegacyWebSavesIfNeeded();
    refreshWebSaveFiles();
    setSavePickerContext('ingame');
  }, [refreshWebSaveFiles, showError]);

  const handleWebLoad = useCallback(async (saveFile: WebSaveFile) => {
    await runMutation(async () => {
      const currentTurn = gameState?.turn?.[0] ?? gameState?.turn ?? 1;
      const currentYear = 1815 + Math.floor((currentTurn - 1) / 4);
      const currentQuarter = ((currentTurn - 1) % 4) + 1;
      if (
        !confirm(
          `Load "${saveFile.name}" for ${saveFile.playerName} (${turnToYearQ(saveFile.turnNumber)})? This will replace the current game state (${currentYear} Q${currentQuarter}).`,
        )
      ) {
        return;
      }
      setSavePickerContext(null);
      setBusyMessage('Loading browser save…');
      try {
        const payload = await getSavePayload(saveFile.id);
        if (!payload) {
          showError(`Save "${saveFile.name}" is missing payload data.`);
          return;
        }
        if (!(await applyGameJson(payload))) return;
        setActiveScreen('map');
        setProvinceUnits(null);
        setSelectedUnitIds([]);
        setIsDeployMode(false);
        setDeployingCivilian(null);
        setDeployableTiles(new Set());
        setProspectedTiles(new Set());
        setSelectedTile(null);
        setSelectedNavyKey(null);
        setHoveredNavyKey(null);
      } finally {
        setBusyMessage(null);
      }
    });
  }, [applyGameJson, gameState, runMutation, showError]);

  const handleOpenSetupLoadPicker = useCallback(async () => {
    await migrateLegacyWebSavesIfNeeded();
    refreshWebSaveFiles();
    setSavePickerContext('setup');
  }, [refreshWebSaveFiles]);

  const handleWebLoadFromSetup = useCallback(async (saveFile: WebSaveFile) => {
    setBusyMessage('Loading browser save…');
    try {
      const payload = await getSavePayload(saveFile.id);
      if (!payload) {
        showError(`Save "${saveFile.name}" is missing payload data.`);
        return;
      }
      const state = parseGameJson(payload);
      if (state?.error) {
        showError(`Save "${saveFile.name}" is invalid.`);
        return;
      }
      const diff = String(state?.difficulty ?? 'Normal');
      const difficulty =
        diff === 'Introductory' ? 0
        : diff === 'Easy' ? 1
        : diff === 'Hard' ? 3
        : diff === 'NighOnImpossible' || diff === 'NOI' ? 4
        : 2;
      const params: GameStartParams = {
        mapKey: state?.world?.map_key ?? 'imperialism',
        observerMode: state?.observer_mode === true,
        scenario: null,
        difficulty,
        nationIdx: 0,
        mapGenConfig: {
          ...DEFAULT_MAP_GEN_CONFIG,
        },
        organicBorders,
        hideHexGrid,
      };
      setSavePickerContext(null);
      await handleGameStart(payload, params);
    } finally {
      setBusyMessage(null);
    }
  }, [handleGameStart, hideHexGrid, organicBorders, showError]);

  const handleDeleteWebSave = useCallback(async (saveFile: WebSaveFile) => {
    if (!confirm(`Delete save "${saveFile.name}"?`)) return;
    try {
      await deleteSavePayload(saveFile.id);
      const next = readWebSaveFiles().filter(s => s.id !== saveFile.id);
      writeWebSaveFiles(next);
      refreshWebSaveFiles();
      if (next.length === 0) setSavePickerContext(null);
    } catch (err) {
      console.error('Failed to delete save:', err);
      showError(`Failed to delete "${saveFile.name}".`);
    }
  }, [refreshWebSaveFiles, showError]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        e.preventDefault();
        if (activeScreen === 'newspaper') {
          dismissNewspaper();
        } else if (!showProposals) {
          handleEndTurn();
        }
      }
      if (e.code === 'Escape') {
        if (savePickerContext) { setSavePickerContext(null); }
        else if (queuedDiplomacyAction) { setQueuedDiplomacyAction(null); }
        else if (selectedUnitIds.length > 0) { setSelectedUnitIds([]); }
        else if (selectedNavyKey) { setSelectedNavyKey(null); }
        else if (pendingEngineerDeploy) { setPendingEngineerDeploy(null); }
        else if (isDeployMode) { setIsDeployMode(false); setDeployingCivilian(null); setDeployableTiles(new Set()); setProspectedTiles(new Set()); }
        else if (showProposals) setShowProposals(false);
        else if (activeScreen === 'newspaper') dismissNewspaper();
        else if (isFullScreen(activeScreen)) setActiveScreen('map');
      }
      if (e.code === 'F1') { e.preventDefault(); setActiveScreen('map'); }
      if (e.code === 'F2') { e.preventDefault(); setActiveScreen('transport'); }
      if (e.code === 'F3') { e.preventDefault(); setActiveScreen('industry'); }
      if (e.code === 'F4') { e.preventDefault(); setActiveScreen('diplomacy'); }
      if (e.code === 'F5') { e.preventDefault(); setActiveScreen('trade'); }
      if (e.code === 'F6') { e.preventDefault(); setActiveScreen('tech'); }
      if (e.code === 'F7') { e.preventDefault(); setActiveScreen('ledger'); }
      if (e.code === 'F8') { e.preventDefault(); setActiveScreen('newspaper'); }
      if (e.code === 'F9') { e.preventDefault(); setActiveScreen('battle'); }
      if (e.code === 'F10') { e.preventDefault(); setActiveScreen('legend'); }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeScreen, showProposals, savePickerContext, handleEndTurn, dismissNewspaper, selectedUnitIds, isDeployMode, pendingEngineerDeploy, queuedDiplomacyAction, selectedNavyKey]);

  // Fetch overlay data when map mode, active screen, or selected nation changes
  useEffect(() => {
    (async () => {
      if (!gameJson || !gameState) return;
      if (mapMode === 'diplomatic' || mapMode === 'relationship' || activeScreen === 'diplomacy') {
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
  }, [mapMode, activeScreen, selectedNation, gameJson, gameState]);

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

  // ── Fleet movement (card #471) ─────────────────────────────────────
  // When the player selects one of their own fleet markers, auto-select
  // every warship in that fleet's sea zone — same UX as auto-selecting
  // capital armies on capital click.
  const isPlayerFleetSelected = !!(
    selectedNavyMarker &&
    selectedNavyMarker.kind === 'fleet' &&
    selectedNavyMarker.nation_id === playerNationId
  );
  useEffect(() => {
    if (!isPlayerFleetSelected || !shipsData || !selectedNavyMarker) return;
    const zoneId = selectedNavyMarker.sea_zone_id ?? null;
    const ids = shipsData.warships
      .filter(s => s.sea_zone === zoneId)
      .map(s => s.id);
    setSelectedShipIds(ids);
  }, [isPlayerFleetSelected, shipsData, selectedNavyMarker]);
  // Adjacent sea hexes the player can click to send the selected fleet there.
  // Driven by the selected marker (not by ship selection) so highlights render
  // even when the auto-select effect hasn't completed yet.
  const validFleetTargets: Set<string> = useMemo(() => {
    const out = new Set<string>();
    if (!isPlayerFleetSelected || !selectedNavyMarker) return out;
    const fromZoneId = selectedNavyMarker.sea_zone_id;
    if (fromZoneId == null) return out;
    const fromZone = seaZones.find(z => z.id === fromZoneId);
    if (!fromZone) return out;
    for (const adjId of fromZone.adjacent_zone_ids ?? []) {
      const adj = seaZones.find(z => z.id === adjId);
      if (!adj || adj.is_lake) continue;
      for (const h of adj.hexes) out.add(`${h.q},${h.r}`);
    }
    return out;
  }, [isPlayerFleetSelected, selectedNavyMarker, seaZones]);

  // Ref so handleTileClick can call handleDeployCivilian without a forward-reference in deps
  const handleDeployCivilianRef = useRef<(civ: CivilianDetail) => void>(() => {});

  const handleTileClick = useCallback(async (tile: TileData) => {
    // Queued diplomacy action: fire the action against the clicked nation.
    if (queuedDiplomacyAction && activeScreen === 'diplomacy') {
      const targetNationId = tile.nation_id;
      if (targetNationId == null || targetNationId === playerNationId) {
        showError('Select a foreign nation for this diplomatic action.');
        return;
      }
      // Block click on a target that can't accept this action; keep queue so user can re-target.
      if (!canTargetNationWithAction(queuedDiplomacyAction, targetNationId, diplomacyScreenData)) {
        return;
      }
      if (mutationLockRef.current) {
        showError('Another action is in progress — please wait.');
        return; // preserve queued action so user can retry
      }
      const action = queuedDiplomacyAction;
      setQueuedDiplomacyAction(null);
      diploActionsRef.current?.(action, targetNationId);
      return;
    }

    // Fleet movement (card #471): if a player fleet is selected and the
    // clicked sea hex sits inside one of the adjacent sea zones, queue a
    // fleet move to that zone. Resolution happens at end-of-turn — same
    // shape as army `pending_moves`. Stays scoped to player fleets — we
    // intentionally do not wire foreign-fleet clicks to mutations.
    if (isPlayerFleetSelected && selectedNavyMarker && validFleetTargets.size > 0) {
      const tileKey = `${tile.q},${tile.r}`;
      if (validFleetTargets.has(tileKey)) {
        const fromZoneId = selectedNavyMarker.sea_zone_id;
        const toZone = seaZones.find(z => z.hexes.some(h => h.q === tile.q && h.r === tile.r));
        if (fromZoneId != null && toZone) {
          await runMutation(async () => {
            const cmd = await moveFleet(gameJson, playerNationId, fromZoneId, toZone.id);
            if (cmd.ok && cmd.gameJson) {
              await applyGameJson(cmd.gameJson);
              // Keep the fleet selected so the player can see the queued
              // move in the sidebar and retarget or cancel it before
              // ending the turn.
              setSelectedShipIds([]);
            } else if (cmd.error) {
              showError(`Fleet move failed: ${cmd.error}`);
            }
          });
        }
        return;
      }
      // Click on a non-target sea hex: fall through and clear selection below.
    }

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
            // Refresh the origin province (selectedTile) — units there now have pending-move markers
            if (provinceUnits && selectedTile?.province_id != null) {
              setProvinceUnits(await getUnitsInProvince(currentJson, selectedTile.province_id));
            }
          }
          setSelectedUnitIds([]);
        });
        return;
      }
      // Invalid target: fall through to normal tile navigation (clears selection below).
    }

    // Deploy mode: clicking a tile deploys the civilian (or prompts engineer action)
    if (isDeployMode && deployingCivilian) {
      // F-004: Only allow clicking highlighted deployable tiles
      const tileKey = `${tile.q},${tile.r}`;
      if (!deployableTiles.has(tileKey)) return; // Ignore click on invalid tile, keep mode active

      if (deployingCivilian.type === 'Engineer') {
        // Show popup to choose what to build before deploying
        setPendingEngineerDeploy({ civ: deployingCivilian, q: tile.q, r: tile.r });
        return;
      }

      await runMutation(async () => {
        let currentJson = gameJson;
        // If the civilian was deployed (idle redeploy), recall from current position first
        if (deployingCivilian.position !== null) {
          const recallCmd = await recallCivilian(currentJson, deployingCivilian.id);
          if (recallCmd.ok && recallCmd.gameJson) {
            currentJson = recallCmd.gameJson;
          } else {
            showError(`Recall failed: ${recallCmd.error ?? 'Unknown error'}`);
            return;
          }
        }
        const cmd = await deployCivilian(currentJson, deployingCivilian.id, tile.q, tile.r);
        if (cmd.ok && cmd.gameJson && (await applyGameJson(cmd.gameJson))) {
          setIsDeployMode(false);
          setDeployingCivilian(null);
          setDeployableTiles(new Set());
          setProspectedTiles(new Set());
        } else if (cmd.error) {
          showError(`Deploy failed: ${cmd.error}`);
        }
      });
      return;
    }

    setSelectedTile(tile);
    setSelectedNavyKey(null);
    setSelectedUnitIds([]);

    // Select civilian when clicking a tile that has a player civilian on it.
    // Idle civilians (working=false) enter deploy mode immediately.
    if (tile.civilian_on_tile?.is_human && tile.nation_id === playerNationId) {
      const cot = tile.civilian_on_tile;
      if (!cot.working) {
        const civ: CivilianDetail = {
          id: cot.id, type: cot.type, working: false, turns_remaining: 0,
          position: { q: tile.q, r: tile.r },
        };
        handleDeployCivilianRef.current(civ);
        return;
      }
      setSelectedCivilianId(cot.id);
    } else {
      setSelectedCivilianId(null);
    }
    if (tile.owner && tile.terrain !== 'Sea' && (mapMode === 'diplomatic' || mapMode === 'relationship')) {
      setSelectedNation(tile.owner);
    }

    // Load province units when clicking a capital tile; auto-select all movable units for player capitals
    if (tile.is_capital && tile.province_id != null) {
      const units = await getUnitsInProvince(gameJson, tile.province_id);
      setProvinceUnits(units);
      if (tile.nation_id === playerNationId && units && units.army_units.length > 0) {
        const selectableIds = units.army_units
          .filter(u => u.category !== 'Garrison')
          .map(u => u.id);
        setSelectedUnitIds(selectableIds);
        // Card #431: switch to Map tab when units are actually selected from another screen
        if (selectableIds.length > 0 && activeScreen !== 'map') setActiveScreen('map');
      } else {
        setSelectedUnitIds([]);
      }
    } else {
      setProvinceUnits(null);
      setSelectedUnitIds([]);
    }
  }, [mapMode, gameJson, playerNationId, selectedUnitIds, validMoveTargets, isDeployMode, deployingCivilian, deployableTiles, applyGameJson, provinceUnits, selectedTile, showError, runMutation, queuedDiplomacyAction, activeScreen, diplomacyScreenData, diploActionsRef, mutationLockRef, isPlayerFleetSelected, selectedNavyMarker, validFleetTargets, seaZones]);

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
    // Only search the player's own nation to avoid collisions with identical unit IDs in other nations
    const playerNation = (gameState?.nations || []).find((n: any) => {
      const nid = typeof n.id === 'number' ? n.id : n.id?.[0] ?? -1;
      return nid === playerNationId;
    });
    return playerMoves.flatMap((m: any) => {
      const unitId = typeof m[1] === 'number' ? m[1] : m[1]?.[0] ?? 0;
      const destId = typeof m[2] === 'number' ? m[2] : m[2]?.[0] ?? 0;
      const unit = playerNation?.military?.army?.find((u: any) => {
        const uid = typeof u.id === 'number' ? u.id : u.id?.[0] ?? -1;
        return uid === unitId;
      });
      if (!unit) return []; // unit not found — skip rather than drawing a misleading arrow
      const sourceId = typeof unit.position === 'number' ? unit.position : unit.position?.[0] ?? 0;
      return [{ unit_id: unitId, source_province_id: sourceId, dest_province_id: destId }];
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
        if (selectedTile?.province_id != null) {
          setProvinceUnits(await getUnitsInProvince(currentJson, selectedTile.province_id));
        }
      }
      if (failed > 0) showError(`Dismissed ${succeeded} of ${n} units \u2014 ${failed} failed`);
    });
  }, [isObserver, selectedUnitIds, gameJson, applyGameJson, selectedTile, showError, runMutation]);

  const handleSetPendingArmyRecruit = useCallback(async (unitType: string, count: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPendingArmyRecruit(gameJson, playerNationId, unitType, count);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Recruit order failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  // Card #417: upgrade a single unit to its next-era variant.
  const handleUpgradeUnit = useCallback(async (unitId: number) => {
    if (isObserver) return;
    await runMutation(async () => {
      const cmd = await upgradeUnit(gameJson, playerNationId, unitId);
      if (cmd.ok && cmd.gameJson && (await applyGameJson(cmd.gameJson))) {
        if (selectedTile?.province_id != null) {
          setProvinceUnits(await getUnitsInProvince(cmd.gameJson, selectedTile.province_id));
        }
      } else if (cmd.error) {
        showError(`Upgrade failed: ${cmd.error}`);
      }
    });
  }, [isObserver, gameJson, playerNationId, applyGameJson, selectedTile, showError, runMutation]);

  // Card #417: bulk-upgrade every selected unit that has an unlocked target.
  // Per-unit failures don't abort the batch (they show up in result.failed);
  // a top-level wasm error short-circuits before we touch game state.
  const handleUpgradeSelected = useCallback(async () => {
    if (isObserver || selectedUnitIds.length === 0) return;
    await runMutation(async () => {
      const result = await upgradeUnits(gameJson, playerNationId, selectedUnitIds);
      if (result.kind === 'error') {
        showError(`Upgrade failed: ${result.error}`);
        return;
      }
      if (await applyGameJson(result.gameJson)) {
        if (selectedTile?.province_id != null) {
          setProvinceUnits(await getUnitsInProvince(result.gameJson, selectedTile.province_id));
        }
        if (result.failed.length > 0) {
          showError(`Upgraded ${result.upgraded} — ${result.failed.length} failed (${result.failed[0].error})`);
        }
      }
    });
  }, [isObserver, gameJson, playerNationId, selectedUnitIds, applyGameJson, selectedTile, showError, runMutation]);

  const handleDeployCivilian = useCallback((civ: CivilianDetail) => {
    if (isObserver) return;
    setDeployingCivilian(civ);
    setIsDeployMode(true);
    // Compute deployable tiles — tiles owned by player where civilian type can work
    const validTiles = new Set<string>();
    const checkedTiles = new Set<string>();
    for (const t of tiles) {
      const ter = t.terrain;
      // For Prospectors: mark already-searched tiles for red-X overlay, even if occupied
      if (civ.type === 'Prospector' && t.nation_id === playerNationId && PROSPECTOR_TERRAIN.has(ter) && t.is_prospected) {
        checkedTiles.add(`${t.q},${t.r}`);
      }
      if (t.nation_id !== playerNationId || t.terrain === 'Sea' || t.civilian_on_tile) continue;
      // Approximate CivilianType::can_improve logic from domain
      // F-012: Only use visible resources (not hidden deposits)
      const res = (t.resource && !t.resource_hidden) ? t.resource : null;
      let canWork = false;
      switch (civ.type) {
        case 'Farmer': canWork = res === 'Grain' || res === 'Fruit' || res === 'Cotton'; break;
        case 'Rancher': canWork = res === 'Wool' || res === 'Livestock' || res === 'Horses'; break;
        case 'Forester': canWork = res === 'Timber'; break;
        case 'Miner': canWork = res === 'Coal' || res === 'Iron'; break;
        case 'Driller': canWork = res === 'Oil'; break;
        case 'Prospector': canWork = PROSPECTOR_TERRAIN.has(ter) && !t.is_prospected; break;
        case 'Engineer': canWork = true; break; // any land tile
      }
      if (canWork) validTiles.add(`${t.q},${t.r}`);
    }
    setDeployableTiles(validTiles);
    setProspectedTiles(checkedTiles);
  }, [tiles, playerNationId]);
  handleDeployCivilianRef.current = handleDeployCivilian;

  const handleRecallCivilian = useCallback(async (civilianId: number): Promise<boolean> => {
    let success = false;
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await recallCivilian(gameJson, civilianId);
      if (cmd.ok && cmd.gameJson) {
        await applyGameJson(cmd.gameJson);
        success = true;
      } else if (cmd.error) {
        showError(`Recall failed: ${cmd.error}`);
      }
    });
    return success;
  }, [isObserver, gameJson, applyGameJson, showError, runMutation]);

  const handleEngineerBuild = useCallback(async (civilianId: number, kind: EngineerBuildKind) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await engineerBuild(gameJson, civilianId, kind);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Engineer build failed: ${cmd.error}`);
    });
  }, [gameJson, applyGameJson, showError, runMutation]);

  // Handle engineer action choice after clicking a deploy tile
  const handleEngineerDeployChoice = useCallback(async (kind: EngineerBuildKind | null) => {
    if (!pendingEngineerDeploy) return;
    const { civ, q, r } = pendingEngineerDeploy;
    setPendingEngineerDeploy(null);
    if (kind === null) return; // cancelled

    await runMutation(async () => {
      if (isObserver) return;
      let currentJson = gameJson;
      // If the engineer was deployed (idle redeploy), recall first
      if (civ.position !== null) {
        const recallCmd = await recallCivilian(currentJson, civ.id);
        if (recallCmd.ok && recallCmd.gameJson) {
          currentJson = recallCmd.gameJson;
        } else {
          showError(`Recall failed: ${recallCmd.error ?? 'Unknown error'}`);
          return;
        }
      }
      const deployCmd = await deployCivilian(currentJson, civ.id, q, r);
      if (!deployCmd.ok || !deployCmd.gameJson) {
        showError(`Deploy failed: ${deployCmd.error ?? 'Unknown error'}`);
        return;
      }
      const buildCmd = await engineerBuild(deployCmd.gameJson, civ.id, kind);
      const finalJson = buildCmd.ok && buildCmd.gameJson ? buildCmd.gameJson : deployCmd.gameJson;
      if (await applyGameJson(finalJson)) {
        setIsDeployMode(false);
        setDeployingCivilian(null);
        setDeployableTiles(new Set());
        setProspectedTiles(new Set());
      }
      if (buildCmd.error) showError(`Build failed: ${buildCmd.error}`);
    });
  }, [pendingEngineerDeploy, gameJson, applyGameJson, showError, runMutation, isObserver]);

  // Selecting a civilian from the sidebar:
  //   - undeployed → enter deploy mode
  //   - deployed + idle (working=false) → enter deploy mode immediately; recall happens on tile click
  //   - deployed + busy → navigate to map and select
  const handleSelectCivilian = useCallback(async (civ: CivilianDetail) => {
    if (isObserver) return;
    if (civ.position) {
      if (!civ.working) {
        // Idle deployed civilian: enter deploy mode right away; tile click will recall+redeploy
        setActiveScreen('map');
        handleDeployCivilian(civ);
      } else {
        // Busy deployed civilian: navigate to map, select tile
        setActiveScreen('map');
        setSelectedCivilianId(civ.id);
        setSelectedNavyKey(null);
        setSelectedUnitIds([]);
        const civTile = tiles.find(t => t.q === civ.position!.q && t.r === civ.position!.r);
        if (civTile) {
          setSelectedTile(civTile);
          if (civTile.is_capital && civTile.province_id != null) {
            setProvinceUnits(await getUnitsInProvince(gameJson, civTile.province_id));
          } else {
            setProvinceUnits(null);
          }
        } else {
          setProvinceUnits(null);
        }
        const HEX_SIZE = 18;
        const SQRT3 = Math.sqrt(3);
        const px = HEX_SIZE * (SQRT3 * civ.position.q + SQRT3 / 2 * civ.position.r);
        const py = HEX_SIZE * (3 / 2 * civ.position.r);
        const mapWidth = window.innerWidth - 300;
        const mapHeight = window.innerHeight;
        setMapOffset({ x: mapWidth / 2 - px * mapScale, y: mapHeight / 2 - py * mapScale });
      }
    } else {
      // Undeployed: enter deploy mode and switch to map
      setActiveScreen('map');
      handleDeployCivilian(civ);
    }
  }, [isObserver, mapScale, tiles, gameJson, handleDeployCivilian]);

  const handleSetPendingCivilianHire = useCallback(async (civType: string, count: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPendingCivilianHire(gameJson, playerNationId, civType, count);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Hire failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetPendingShips = useCallback(async (shipType: string, count: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPendingShips(gameJson, playerNationId, shipType, count);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Ship order failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetAutoTradeWithMinors = useCallback(async (enabled: boolean) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setAutoTradeWithMinors(gameJson, playerNationId, enabled);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Auto-trade toggle failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  // ── New screen handlers ──────────────────────────────────────────

  const handleSetPendingFreightCars = useCallback(async (count: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPendingFreightCars(gameJson, playerNationId, count);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Freight car failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetAllocation = useCallback(async (resource: string, units: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setTransportAllocation(gameJson, playerNationId, resource, units);
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

  const handleSetChainTarget = useCallback(async (chain: string, step: string, target: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setChainTarget(gameJson, playerNationId, chain, step, target);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Chain target failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetPendingTraining = useCallback(async (toTrained: number, toExpert: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPendingTraining(gameJson, playerNationId, toTrained, toExpert);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Training failed: ${cmd.error}`);
    });
  }, [gameJson, playerNationId, applyGameJson, showError, runMutation]);

  const handleSetPendingImmigration = useCallback(async (count: number) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await setPendingImmigration(gameJson, playerNationId, count);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Immigration failed: ${cmd.error}`);
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

  // Keep diploActionsRef up to date on every render so handleTileClick can dispatch without forward-referencing these handlers
  diploActionsRef.current = (action: QueuedDiplomacyAction, targetId: number) => {
    switch (action.kind) {
      case 'consulate': handleDiploBuildConsulate(targetId); break;
      case 'embassy': handleDiploBuildEmbassy(targetId); break;
      case 'nap': handleDiploProposeNap(targetId); break;
      case 'alliance': handleDiploProposeAlliance(targetId); break;
      case 'peace': handleDiploProposePeace(targetId); break;
      case 'grant': handleDiploSendGrant(targetId, action.amount); break;
      case 'breakTreaty': handleDiploBreakTreaty(targetId, action.treatyType); break;
      case 'war': handleDiploDeclareWar(targetId); break;
    }
  };

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
        <div style={{ marginTop: 6, paddingTop: 6, borderTop: '1px solid #3a3520', fontSize: 'var(--ui-font-size, 14px)' }}>
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
        <div style={{ marginTop: 6, paddingTop: 6, borderTop: '1px solid #3a3520', fontSize: 'var(--ui-font-size, 14px)' }}>
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

  const handleQueueTech = useCallback(async (techName: string) => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await queueTechResearch(gameJson, techName);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Queue tech failed: ${cmd.error}`);
    });
  }, [gameJson, applyGameJson, showError, runMutation, isObserver]);

  const handleCancelTech = useCallback(async () => {
    await runMutation(async () => {
      if (isObserver) return;
      const cmd = await cancelTechResearch(gameJson);
      if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
      else if (cmd.error) showError(`Cancel tech failed: ${cmd.error}`);
    });
  }, [gameJson, applyGameJson, showError, runMutation, isObserver]);

  if (loading) return <div style={styles.loading}>Loading Imperialism...</div>;
  if (wasmError) return (
    <div style={{color: '#e63946', padding: '2rem', fontFamily: 'Georgia, serif'}}>
      <h2>Initialization Error</h2>
      <p>{wasmError}</p>
      <p>Try refreshing the page. If the problem persists, check that WebAssembly is enabled in your browser.</p>
    </div>
  );
  if (!gameStarted) return (
    <>
      <GameSetup
        onStartGame={handleGameStart}
        onRequestLoadSavedGame={handleOpenSetupLoadPicker}
      />
      <BusyOverlay
        busy={busyMessage !== null}
        message={busyMessage ?? undefined}
        cancellable={false}
      />
      {savePickerContext && (
        <div style={styles.modal}>
          <div style={{ ...styles.modalContent, width: 680 }}>
            <h2 style={{ marginTop: 0, color: '#daa520' }}>Load Save File</h2>
            {webSaveFiles.length === 0 ? (
              <p style={styles.hint}>No saved files yet. Use Save first.</p>
            ) : (
              <div style={{ maxHeight: '55vh', overflowY: 'auto' }}>
                {webSaveFiles.map(saveFile => (
                  <div
                    key={saveFile.id}
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      gap: 10,
                      padding: '8px 10px',
                      marginBottom: 8,
                      border: '1px solid #3a3520',
                      background: 'rgba(255,255,255,0.03)',
                    }}
                  >
                    <div style={{ minWidth: 0 }}>
                      <div style={{ fontWeight: 'bold', color: '#e0d8c0' }}>{saveFile.name}</div>
                      <div style={{ fontSize: 12, color: '#9a9a9a' }}>
                        {saveFile.playerName} · {turnToYearQ(saveFile.turnNumber)} · {new Date(saveFile.savedAtIso).toLocaleString()}
                      </div>
                    </div>
                    <div style={{ display: 'flex', gap: 8, flexShrink: 0 }}>
                      <button onClick={() => handleWebLoadFromSetup(saveFile)} style={styles.btn}>Load</button>
                      <button
                        onClick={() => handleDeleteWebSave(saveFile)}
                        style={{ ...styles.btn, background: '#5a2620', borderColor: '#7b332b' }}
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 12 }}>
              <button onClick={() => setSavePickerContext(null)} style={styles.btn}>Close</button>
            </div>
          </div>
        </div>
      )}
    </>
  );

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
    <main style={{ ...styles.container, '--ui-font-size': `${uiFontSize}px`, fontSize: `var(--ui-font-size, 14px)` } as React.CSSProperties}>
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
        <button onClick={() => setActiveScreen('newspaper')} style={styles.btn}>History</button>
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
        <button
          onClick={handleWebSave}
          style={styles.btn}
          title="Save current game state in this browser"
        >
          Save
        </button>
        <button
          onClick={handleOpenWebLoadPicker}
          style={styles.btn}
          title="Load a saved game from browser storage"
        >
          Load
        </button>
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
        <div style={{ ...styles.mapContainer, display: isFullScreen(activeScreen) ? 'none' : 'flex', flexDirection: 'column' }}>
          <div style={{ flex: 1, position: 'relative', minHeight: 0 }}>
            <HexMap
              tiles={tiles}
              mapMode={mapMode}
              diplomacyOverlay={diplomacyOverlay}
              militaryOverlay={militaryOverlay}
              onMapModeChange={setMapMode}
              onTileClick={handleTileClick}
              showHiddenResources={showHiddenResources}
              showAiCivilians={showAiCivilians}
              showResources={showResources}
              showTransportNetwork={showTransportNetwork}
              showArmies={showArmies}

              pendingMoves={pendingMoveArrows}
              validMoveTargets={validMoveTargets}
              isMovementMode={isMovementMode}
              isDeployMode={isDeployMode}
              deployableTiles={deployableTiles}
              prospectedTiles={prospectedTiles}
              selectedCivilianId={selectedCivilianId}
              disableFogOfWar={disableFogOfWar}
              organicBorders={organicBorders}
              hideHexGrid={hideHexGrid}
              scale={mapScale}
              offset={mapOffset}
              onScaleChange={setMapScale}
              onOffsetChange={setMapOffset}
              navyMarkers={navyMarkers}
              seaZones={seaZones}
              selectedNavyKey={selectedNavyKey}
              onNavyMarkerClick={handleNavyMarkerClick}
              onNavyMarkerHover={handleNavyMarkerHover}
              validFleetTargets={validFleetTargets}
              onTileHover={activeScreen === 'diplomacy' ? setHoveredDiploTile : undefined}
              renderTooltipModeExtras={renderTooltipModeExtras}
              governmentTitleByNationId={governmentTitleByNationId}
              selectedTileKey={selectedTile ? `${selectedTile.q},${selectedTile.r}` : null}
              lockZoom={activeScreen === 'diplomacy'}
              showDiplomacyMarkers={mapMode === 'diplomatic'}
              isDiplomacyTargetMode={activeScreen === 'diplomacy' && queuedDiplomacyAction != null}
              isDiplomacyTargetInvalid={
                activeScreen === 'diplomacy'
                && queuedDiplomacyAction != null
                && hoveredDiploTile != null
                && (
                  hoveredDiploTile.nation_id == null
                  || hoveredDiploTile.nation_id === playerNationId
                  || !canTargetNationWithAction(queuedDiplomacyAction, hoveredDiploTile.nation_id, diplomacyScreenData)
                )
              }
              diplomacyInvalidReason={
                (activeScreen === 'diplomacy' && queuedDiplomacyAction != null && hoveredDiploTile != null)
                  ? diplomacyInvalidReasonFor(queuedDiplomacyAction, hoveredDiploTile.nation_id, playerNationId, diplomacyScreenData)
                  : null
              }
            />
          </div>
          {activeScreen === 'diplomacy' && diplomacyScreenData && (
            <DiplomacyBottomBar
              diplomacy={diplomacyScreenData}
              hoveredNationId={hoveredDiploTile?.nation_id ?? null}
              selectedNationId={selectedTile?.nation_id ?? null}
              playerNationId={playerNationId}
              playerStanding={diplomacyScreenData.player_standing}
              queuedAction={queuedDiplomacyAction}
              onQueue={setQueuedDiplomacyAction}
            />
          )}
        </div>

        {/* Full-screen views */}
        {activeScreen === 'industry' && (
          <div style={{ flex: 1, overflowY: 'auto', background: '#161625', padding: 16 }}>
            {industryData ? (
              <IndustryPanel
                industry={industryData}
                buildable={buildable}
                onExpand={handleExpandBuilding}
                onSetPendingArmyRecruit={handleSetPendingArmyRecruit}
                onSetPendingShips={handleSetPendingShips}
                onSetPendingCivilianHire={handleSetPendingCivilianHire}
                onSetPendingFreightCars={handleSetPendingFreightCars}
                onSetChainTarget={handleSetChainTarget}
                onSetPendingImmigration={handleSetPendingImmigration}
                onSetPendingTraining={handleSetPendingTraining}
              />
            ) : (
              <p style={styles.hint}>Loading industry data...</p>
            )}
          </div>
        )}
        {activeScreen === 'tech' && techScreenData && (
          <TechScreen
            data={techScreenData}
            year={year}
            isObserver={isObserver}
            onQueue={handleQueueTech}
            onCancel={handleCancelTech}
            onClose={() => setActiveScreen('map')}
          />
        )}
        {activeScreen === 'ledger' && (
          <LedgerPanel entries={gpLedgerData} previousEntries={prevGpLedgerData} nations={gameState?.nations || []} onClose={() => setActiveScreen('map')} />
        )}
        {activeScreen === 'newspaper' && (() => {
          const countryOptions: { id: number; name: string }[] = (gameState?.nations || [])
            .filter((n: any) => !!n.name)
            .map((n: any) => ({ id: n.id as number, name: n.name as string }));
          const archive = archiveData;
          return (
            <NewspaperScreen
              playerName={playerName}
              year={year}
              quarter={quarter}
              turnNumber={turnNumber}
              headlines={headlines}
              archiveData={archive}
              archiveLoadState={archiveLoadState}
              nations={gameState?.nations || []}
              countryOptions={countryOptions}
              newsFilterCategory={newsFilterCategory}
              newsFilterCountry={newsFilterCountry}
              newsFilterText={newsFilterText}
              showAiReasoning={showAiReasoning}
              showAiNonActions={showAiNonActions}
              onCategoryChange={setNewsFilterCategory}
              onCountryChange={setNewsFilterCountry}
              onTextChange={setNewsFilterText}
              onRequestArchive={loadNewspaperArchive}
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
            nations={gameState?.nations || []}
            merchants={shipsData?.merchants ?? []}
            onSetSubsidy={handleSetSubsidy}
            onSetSellOrder={handleSetSellOrder}
            onSetBuyOrder={handleSetBuyOrder}
            onSetAutoTradeWithMinors={handleSetAutoTradeWithMinors}
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
            nations={gameState?.nations || []}
            onClose={() => setActiveScreen('map')}
            showRetreatDebug={showRetreatDebug}
            showFirepower={showBattleFirepower}
          />
        )}
        {activeScreen === 'legend' && (
          <LegendScreen
            nations={gameState?.nations || []}
            onClose={() => setActiveScreen('map')}
          />
        )}

        {/* Side panel — context-sensitive, hidden for full-screen views and diplomacy (which uses a bottom bar) */}
        {!isFullScreen(activeScreen) && activeScreen !== 'diplomacy' && (
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
                    <p><b>{selectedTile.terrain}{showResource ? ` — ${resourceLabel(selectedTile.resource!)}` : ''}</b></p>
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
                    fontSize: 'var(--ui-font-size, 14px)', padding: '8px 0', borderTop: '1px solid #3a3520',
                    marginTop: 6, opacity: isSelected ? 1 : 0.85,
                  }}>
                    <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#888', marginBottom: 4 }}>
                      {isSelected ? 'Selected navy' : 'Hovering navy'}
                    </div>
                    <div style={{ color: marker.kind === 'beachhead' ? '#ff8059' : '#e0d8c0' }}>
                      <b>{title}</b>
                    </div>
                    <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#bbb', marginTop: 4 }}>
                      {marker.ship_count} ships &middot; {marker.total_fp} FP &middot; {marker.total_hull} hull
                    </div>
                    {byType.length > 0 && (
                      <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#bbb', marginTop: 4 }}>
                        {byType.map(([t, n]) => `${n} ${t}`).join(', ')}
                      </div>
                    )}
                    {byOp.length > 0 && (
                      <div style={{ fontSize: 'var(--ui-font-size, 14px)', color: '#888', marginTop: 2 }}>
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
                  <div style={{ fontSize: 'var(--ui-font-size, 14px)', padding: '6px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
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
                  <div style={{ fontSize: 'var(--ui-font-size, 14px)', padding: '6px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
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
                      <div style={{ marginTop: 4, fontSize: 'var(--ui-font-size, 14px)', color: '#bbb' }}>
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
                <div style={{ fontSize: 'var(--ui-font-size, 14px)', padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
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
                <div style={{ fontSize: 'var(--ui-font-size, 14px)', padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                  <div style={{ color: '#888', marginBottom: 4 }}>Relationship Score</div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span>-100</span>
                    <div style={{ flex: 1, height: 12, background: 'linear-gradient(to right, rgb(220,40,40), rgb(160,160,160) 50%, rgb(40,200,40))', borderRadius: 2 }} />
                    <span>+100</span>
                  </div>
                </div>
              )}
              {mapMode === 'military' && (
                <div style={{ fontSize: 'var(--ui-font-size, 14px)', padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
                  <div style={{ color: '#888', marginBottom: 4 }}>Army Strength (vs average)</div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span>Weak</span>
                    <div style={{ flex: 1, height: 12, background: 'linear-gradient(to right, rgb(220,40,40), rgb(200,200,40) 50%, rgb(40,200,40))', borderRadius: 2 }} />
                    <span>Strong</span>
                  </div>
                </div>
              )}
              {mapMode === 'naval' && (
                <div style={{ fontSize: 'var(--ui-font-size, 14px)', padding: '8px 0', borderTop: '1px solid #3a3520', marginTop: 6 }}>
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
                <div style={{ background: 'rgba(200,50,50,0.2)', border: '1px solid rgba(200,50,50,0.5)', borderRadius: 4, padding: 8, marginBottom: 8, fontSize: 'var(--ui-font-size, 14px)', color: '#f88' }}>
                  {statusMessage}
                </div>
              )}

              {/* Movement/Deploy mode indicator */}
              {isMovementMode && (
                <div style={{ background: 'rgba(255,200,0,0.15)', border: '1px solid rgba(255,200,0,0.4)', borderRadius: 4, padding: 8, marginBottom: 8, fontSize: 'var(--ui-font-size, 14px)' }}>
                  <b>Movement Mode</b> — {selectedUnitIds.length > 1 ? `moving ${selectedUnitIds.length} units` : 'moving 1 unit'} — click a highlighted province, or press Escape to cancel.
                </div>
              )}
              {isDeployMode && deployingCivilian && !pendingEngineerDeploy && (
                <div style={{ background: 'rgba(46,204,64,0.15)', border: '1px solid rgba(46,204,64,0.4)', borderRadius: 4, padding: 8, marginBottom: 8, fontSize: 'var(--ui-font-size, 14px)' }}>
                  <b>Deploy {deployingCivilian.type}</b> — click a highlighted tile, or press Escape to cancel.
                </div>
              )}
              {pendingEngineerDeploy && (
                <div style={{ background: 'rgba(46,100,200,0.2)', border: '1px solid rgba(46,100,200,0.5)', borderRadius: 4, padding: 10, marginBottom: 8, fontSize: 'var(--ui-font-size, 14px)' }}>
                  <div style={{ fontWeight: 'bold', marginBottom: 6 }}>
                    🔧 Engineer at ({pendingEngineerDeploy.q},{pendingEngineerDeploy.r}) — what to build?
                  </div>
                  <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' as const }}>
                    {(['railroad', 'depot', 'port'] as const).map(kind => (
                      <button key={kind} onClick={() => handleEngineerDeployChoice(kind)}
                        style={{ background: '#364', color: '#fff', border: 'none', borderRadius: 3, padding: '3px 10px', fontSize: 11, cursor: 'pointer' }}>
                        {kind.charAt(0).toUpperCase() + kind.slice(1)}
                      </button>
                    ))}
                    <button onClick={() => handleEngineerDeployChoice(null)}
                      style={{ background: '#555', color: '#ddd', border: 'none', borderRadius: 3, padding: '3px 10px', fontSize: 11, cursor: 'pointer' }}>
                      Cancel
                    </button>
                  </div>
                </div>
              )}

              {/* Unit Panel — shown when a capital with units is selected */}
              {provinceUnits && (
                <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
                  <UnitPanel
                    provinceUnits={provinceUnits}
                    pendingMoves={pendingMovesDisplay}
                    isPlayerProvince={isPlayerProvince}
                    selectedUnitIds={selectedUnitIds}
                    onToggleUnit={handleToggleUnit}
                    onSelectAll={handleSelectAll}
                    onCancelMove={handleCancelMove}
                    onCancelSelectedMoves={handleCancelSelectedMoves}
                    onDismissSelected={handleDismissSelected}
                    onUpgradeUnit={handleUpgradeUnit}
                    onUpgradeSelected={handleUpgradeSelected}
                    showHealDebug={showHealDebug}
                  />
                </div>
              )}

              {/* Civilian Panel — shown for player when any player tile selected */}
              {civilians && isPlayerProvince && (
                <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
                  <CivilianPanel
                    civilians={civilians}
                    selectedCivilianId={selectedCivilianId}
                    onSelectCivilian={handleSelectCivilian}
                  />
                </div>
              )}

              {/* Selected civilian banner — recall action (no persistent sidebar buttons) */}
              {selectedCivilianId != null && civilians && (() => {
                const selCiv = civilians.deployed.find(c => c.id === selectedCivilianId);
                if (!selCiv) return null;
                return (
                  <div style={{ background: 'rgba(255,255,255,0.08)', border: '1px solid rgba(255,255,255,0.18)', borderRadius: 4, padding: '6px 8px', marginBottom: 6, fontSize: 'var(--ui-font-size, 14px)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ color: '#bbb' }}>Selected: {selCiv.type}</span>
                    <button
                      onClick={async () => { if (await handleRecallCivilian(selCiv.id)) setSelectedCivilianId(null); }}
                      style={{ background: '#a63', color: '#fff', border: 'none', borderRadius: 3, padding: '2px 8px', fontSize: 10, cursor: 'pointer' }}
                    >
                      Recall
                    </button>
                  </div>
                );
              })()}

              {/* Naval Panel — shown at country capital, or when a player fleet
                  marker is selected (card #471). When a fleet is selected the
                  panel is interactive: ships can be toggled and clicking an
                  adjacent sea hex queues a fleet move (resolved at end of
                  turn). The panel surfaces any pending move with a Cancel
                  button, mirroring the army UnitPanel pattern. */}
              {shipsData && (isPlayerCapital || isPlayerFleetSelected) && (
                <div style={{ borderTop: '1px solid #3a3520', paddingTop: 8, marginTop: 6 }}>
                  <NavalPanel
                    ships={shipsData}
                    selectedNavyMarker={isPlayerFleetSelected ? selectedNavyMarker : null}
                    selectedShipIds={isPlayerFleetSelected ? selectedShipIds : []}
                    pendingMoveDestZone={(() => {
                      if (!isPlayerFleetSelected || !selectedNavyMarker?.pending_move_to_zone_id) return null;
                      const dest = seaZones.find(z => z.id === selectedNavyMarker.pending_move_to_zone_id);
                      return dest ? { id: dest.id, name: dest.name } : null;
                    })()}
                    onCancelPendingMove={isPlayerFleetSelected && selectedNavyMarker?.sea_zone_id != null ? async () => {
                      const fromZoneId = selectedNavyMarker.sea_zone_id!;
                      await runMutation(async () => {
                        const cmd = await cancelFleetMove(gameJson, playerNationId, fromZoneId);
                        if (cmd.ok && cmd.gameJson) await applyGameJson(cmd.gameJson);
                        else if (cmd.error) showError(`Cancel failed: ${cmd.error}`);
                      });
                    } : undefined}
                    onToggleShip={isPlayerFleetSelected ? (id) => {
                      setSelectedShipIds(prev =>
                        prev.includes(id) ? prev.filter(i => i !== id) : [...prev, id]
                      );
                    } : undefined}
                    onSelectAll={isPlayerFleetSelected && selectedNavyMarker ? () => {
                      const zoneId = selectedNavyMarker.sea_zone_id ?? null;
                      const all = shipsData.warships
                        .filter(s => s.sea_zone === zoneId)
                        .map(s => s.id);
                      setSelectedShipIds(prev => (prev.length === all.length ? [] : all));
                    } : undefined}
                  />
                </div>
              )}

              <h3 style={styles.panelTitle}>UI</h3>
              <div style={{ padding: '4px 0', fontSize: 'var(--ui-font-size, 14px)', display: 'flex', flexDirection: 'column' as const, gap: 4 }}>
                <label>
                  <input type="checkbox" checked={organicBorders} onChange={e => setOrganicBorders(e.target.checked)} />
                  {' '}Organic borders
                </label>
                <label>
                  <input type="checkbox" checked={hideHexGrid} onChange={e => setHideHexGrid(e.target.checked)} />
                  {' '}Hide hex grid
                </label>
                <label>
                  <input type="checkbox" checked={showResources} onChange={e => setShowResources(e.target.checked)} />
                  {' '}Show resources
                </label>
                <label>
                  <input type="checkbox" checked={showTransportNetwork} onChange={e => setShowTransportNetwork(e.target.checked)} />
                  {' '}Show transport network
                </label>
                <label>
                  <input type="checkbox" checked={showArmies} onChange={e => setShowArmies(e.target.checked)} />
                  {' '}Show armies
                </label>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <span>Font:</span>
                  <input
                    type="range"
                    min={10}
                    max={20}
                    value={uiFontSize}
                    onChange={e => setUiFontSize(parseInt(e.target.value))}
                    style={{ flex: 1, cursor: 'pointer' }}
                  />
                  <span style={{ minWidth: 28, textAlign: 'right' }}>{uiFontSize}px</span>
                </div>
              </div>

              <h3 style={styles.panelTitle}>Debug</h3>
              <div style={{ padding: '4px 0', fontSize: 'var(--ui-font-size, 14px)', display: 'flex', flexDirection: 'column' as const, gap: 4 }}>
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
                  <input type="checkbox" checked={showPersonalities} onChange={e => setShowPersonalities(e.target.checked)} />
                  {' '}Show AI personalities
                </label>
                <label>
                  <input type="checkbox" checked={disableFogOfWar} onChange={e => setDisableFogOfWar(e.target.checked)} />
                  {' '}Disable fog of war
                </label>
                <label>
                  <input type="checkbox" checked={showHealDebug} onChange={e => setShowHealDebug(e.target.checked)} />
                  {' '}Show heal-blocker reasons (units)
                </label>
                <label>
                  <input type="checkbox" checked={showRetreatDebug} onChange={e => setShowRetreatDebug(e.target.checked)} />
                  {' '}Show retreat math (battles)
                </label>
                <label>
                  <input type="checkbox" checked={showBattleFirepower} onChange={e => setShowBattleFirepower(e.target.checked)} />
                  {' '}Show firepower in battle screen
                </label>
              </div>

              <h3 style={styles.panelTitle}>Nations</h3>
              <div style={styles.nationList}>
                {gameState?.nations?.filter((n: any) => n.nation_type === 'GreatPower').map((n: any) => (
                  <div key={n.id} style={styles.nationItem}>
                    <span>{n.name}</span>
                    <span style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                      {showPersonalities && n.diplomacy?.ai_personality && (
                        <span style={{ fontSize: 10, color: '#888', fontStyle: 'italic' }}>{n.diplomacy.ai_personality}</span>
                      )}
                      <span>{n.province_ids?.length || 0} prov</span>
                    </span>
                  </div>
                ))}
              </div>
            </>
          )}
          {activeScreen === 'transport' && (
            transportData ? (
              <TransportPanel
                transport={transportData}
                onSetAllocation={handleSetAllocation}
              />
            ) : (
              <p style={styles.hint}>Loading transport data...</p>
            )
          )}
          {/* Diplomacy sidebar replaced by DiplomacyBottomBar inside mapContainer */}
        </div>
        )}
      </div>{/* end mainArea */}

      {/* Global error toast — visible across all screens including full-screen views */}
      {statusMessage && (
        <div style={{
          position: 'fixed', bottom: 20, left: '50%', transform: 'translateX(-50%)',
          background: 'rgba(200,50,50,0.95)', border: '1px solid rgba(255,80,80,0.8)',
          borderRadius: 6, padding: '10px 20px', fontSize: 'var(--ui-font-size, 14px)', color: '#fff',
          zIndex: 200, maxWidth: 500, textAlign: 'center',
          boxShadow: '0 4px 12px rgba(0,0,0,0.4)',
        }}>
          {statusMessage}
        </div>
      )}

      {savePickerContext === 'ingame' && (
        <div style={styles.modal}>
          <div style={{ ...styles.modalContent, width: 680 }}>
            <h2 style={{ marginTop: 0, color: '#daa520' }}>Load Save File</h2>
            {webSaveFiles.length === 0 ? (
              <p style={styles.hint}>No saved files yet. Use Save first.</p>
            ) : (
              <div style={{ maxHeight: '55vh', overflowY: 'auto' }}>
                {webSaveFiles.map(saveFile => (
                  <div
                    key={saveFile.id}
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      gap: 10,
                      padding: '8px 10px',
                      marginBottom: 8,
                      border: '1px solid #3a3520',
                      background: 'rgba(255,255,255,0.03)',
                    }}
                  >
                    <div style={{ minWidth: 0 }}>
                      <div style={{ fontWeight: 'bold', color: '#e0d8c0' }}>{saveFile.name}</div>
                      <div style={{ fontSize: 12, color: '#9a9a9a' }}>
                        {saveFile.playerName} · {turnToYearQ(saveFile.turnNumber)} · {new Date(saveFile.savedAtIso).toLocaleString()}
                      </div>
                    </div>
                    <div style={{ display: 'flex', gap: 8, flexShrink: 0 }}>
                      <button onClick={() => handleWebLoad(saveFile)} style={styles.btn}>Load</button>
                      <button
                        onClick={() => handleDeleteWebSave(saveFile)}
                        style={{ ...styles.btn, background: '#5a2620', borderColor: '#7b332b' }}
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 12 }}>
              <button onClick={() => setSavePickerContext(null)} style={styles.btn}>Close</button>
            </div>
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

      <BusyOverlay
        busy={busyMessage !== null}
        message={busyMessage ?? undefined}
        cancellable={skipCancellable}
        onCancel={() => { skipCancelRef.current = true; }}
      />
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
  screenTab: { flex: 1, padding: '10px 8px', textAlign: 'center' as const, fontSize: 'var(--ui-font-size, 14px)', color: '#9a9a9a', background: 'none', border: 'none', cursor: 'pointer', fontFamily: 'Georgia, serif', borderBottom: '3px solid transparent', display: 'flex', flexDirection: 'column' as const, alignItems: 'center' as const },
  screenTabActive: { color: '#daa520', borderBottomColor: '#daa520', background: 'rgba(218,165,32,0.05)' },
  hotkey: { fontSize: 10, color: '#555', display: 'block', marginTop: 2 },
  hotkeyActive: { fontSize: 10, color: '#8a7530', display: 'block', marginTop: 2 },
  mainArea: { display: 'flex', flex: 1, overflow: 'hidden', minHeight: 0 },
  mapContainer: { flex: 1, background: '#0a0a1a', minHeight: 0, position: 'relative' as const, display: 'flex' as const },
  sidePanel: { width: 260, padding: 12, background: '#161625', borderLeft: '2px solid #3a3520', overflowY: 'auto' as const, flexShrink: 0 },
  panelTitle: { margin: '12px 0 6px', color: '#daa520', borderBottom: '1px solid #3a3520', paddingBottom: 4 },
  tileInfo: { fontSize: 'var(--ui-font-size, 14px)' },
  tileOwnerRow: { display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 },
  tileOwnerName: { fontWeight: 'bold', color: '#daa520' },
  tileSelected: { background: 'rgba(218,165,32,0.1)', border: '1px solid rgba(218,165,32,0.3)', borderRadius: 4, padding: 8, marginBottom: 8 },
  tileHovered: { padding: 8, marginBottom: 8, opacity: 0.8 },
  tileLabel: { fontSize: 11, color: '#daa520', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  tileLabelDim: { fontSize: 11, color: '#888', textTransform: 'uppercase' as const, letterSpacing: 0.5, marginBottom: 4 },
  hint: { color: '#9a9a9a', fontStyle: 'italic' },
  nationList: { fontSize: 'var(--ui-font-size, 14px)' },
  nationItem: { display: 'flex', justifyContent: 'space-between', padding: '2px 0' },
  btn: { padding: '4px 12px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', cursor: 'pointer', fontFamily: 'Georgia, serif' },
  endTurnBtn: { padding: '6px 20px', background: '#8b4513', color: '#fff', border: '1px solid #a0522d', cursor: 'pointer', fontWeight: 'bold', fontFamily: 'Georgia, serif' },
  skipInput: { width: 48, padding: '4px 6px', background: '#1a1a2e', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif' },
  skipUntilInput: { width: 110, padding: '4px 6px', background: '#1a1a2e', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif' },
  viewpointSelect: { padding: '4px 8px', background: '#3a3520', color: '#e0d8c0', border: '1px solid #5a5030', fontFamily: 'Georgia, serif', cursor: 'pointer' },
  mapKeyChip: { padding: '2px 8px', background: '#1a1a2e', color: '#9a9a9a', border: '1px solid #3a3520', fontFamily: 'monospace', fontSize: 'var(--ui-font-size, 14px)', cursor: 'pointer', userSelect: 'none' as const },
  modal: { position: 'fixed' as const, inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', justifyContent: 'center', alignItems: 'center', zIndex: 100 },
  modalContent: { background: '#1a1a2e', border: '2px solid #daa520', padding: 24, maxWidth: 500, maxHeight: '80vh', overflowY: 'auto' as const },
  headline: { margin: '6px 0', fontSize: 'var(--ui-font-size, 14px)' },
};

export default App;
