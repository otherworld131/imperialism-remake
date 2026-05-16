import { call, callWithTimeout } from './workers/gameClient';

// initWasm is a no-op retained for API compatibility. The dedicated worker
// initializes the wasm module lazily on its first RPC call.
export async function initWasm(): Promise<void> {
  await call<string>('wasm_get_scenarios');
}

export type MapMode = 'terrain' | 'political' | 'diplomatic' | 'relationship' | 'military' | 'naval';

export interface TileData {
  q: number; r: number;
  terrain: string; resource: string | null; resource_hidden: boolean;
  is_capital: boolean;
  is_country_capital: boolean;
  improvement_level: number;
  max_improvement_level: number;
  owner: string; owner_color: string; province: string;
  province_id: number | null;
  has_railroad: boolean; has_depot: boolean; has_port: boolean;
  port_blockaded: boolean;
  has_fort: boolean; fort_level: number;
  map_width: number;
  map_height: number;
  nation_id: number;
  army_firepower: number;
  army_unit_count: number;
  army_composition: Record<string, number> | null;
  naval_firepower: number;
  naval_ship_count: number;
  civilian_on_tile: { id: number; type: string; working: boolean; turns_remaining: number; build_task: string | null; owner: string; owner_color: string; is_human: boolean } | null;
  is_minor: boolean;
  is_incorporated_minor: boolean;
  /** When the tile's province was diplomatically incorporated into the
   *  owner GP, this is the original minor's nation_id (kept for display so
   *  the UI can still show the absorbed nation's name + flag). */
  incorporated_nation_id: number | null;
  is_anarchic: boolean;
  visual_group: string | null;
  visible: boolean;
  is_prospected: boolean;
}

export interface NavyMarker {
  q: number;
  r: number;
  nation_id: number;
  owner_name: string;
  owner_color: string;
  kind: 'fleet' | 'beachhead';
  target_province?: string;
  target_hex?: { q: number; r: number };
  ship_count: number;
  total_fp: number;
  total_hull: number;
  by_type: Record<string, number>;
  by_operation: Record<string, number>;
  visible: boolean;
  sea_zone_id?: number;
  sea_zone_name?: string;
  /** Destination sea-zone id when a fleet move is queued from this marker
   *  (card #471). Drained at end of turn by the domain processor. */
  pending_move_to_zone_id?: number;
}

export interface SeaZone {
  id: number;
  name: string;
  is_lake: boolean;
  center_q: number;
  center_r: number;
  hexes: { q: number; r: number }[];
  adjacent_zone_ids: number[];
}

export interface Headline {
  text: string;
  category: 'war' | 'battle' | 'diplomacy' | 'growth' | 'trade' | 'crisis' | 'politics' | 'military' | 'default';
  reason?: string;
  is_non_action?: boolean;
  nation_ids?: number[];
}

export interface ArchivedNewspaper {
  turn: number;
  year: number;
  quarter: number;
  headlines: Headline[];
}

export interface PoliticalSnapshotTile {
  q: number; r: number;
  terrain: string;
  owner: string;
  owner_color: string;
  province: string;
  is_capital: boolean;
  is_country_capital: boolean;
  is_minor: boolean;
  is_incorporated_minor: boolean;
  visual_group: string | null;
}

export interface PoliticalSnapshot {
  turn: number;
  year: number;
  quarter: number;
  map_width: number;
  map_height: number;
  tiles: PoliticalSnapshotTile[];
}

export interface BattleTile {
  q: number;
  r: number;
}

export interface MedalAward {
  unit_type: string;
  medals: number;
}

export interface BattleUnit {
  unit_type: string;
  health: number;
  medals: number;
  effective_firepower: number;
}

/**
 * Per-defender-unit bonus breakdown — captured at battle start so the UI
 * can explain how a unit's raw FP turns into its contribution to
 * `defender_initial_fp`. Card #478 simplified model:
 *   contribution = applied_firepower × fort_multiplier + entrenchment_fp
 * `applied_firepower` already accounts for the role-aware artillery
 * melee penalty. `null` for attacker logs.
 */
export interface DefenderBonusBreakdown {
  applied_firepower: number;
  fort_multiplier: number;
  entrenchment_fp: number;
  initial_total_contribution: number;
}

/** Per-unit log for a battle — survivors and destroyed alike. */
export interface BattleUnitLog {
  unit_type: string;
  medals_initial: number;
  medals_final: number;
  initial_health: number;
  final_health: number;
  initial_firepower: number;
  final_firepower: number;
  defender_breakdown: DefenderBonusBreakdown | null;
}

/**
 * Per-round trace of how the battle played out — surfaced in the battle
 * screen behind "Show firepower". `round = 0` is the optional first-strike
 * volley (one of the sides shoots, the other doesn't); `round = 1..N` are
 * the regular damage exchanges.
 */
export interface BattleRoundLog {
  round: number;
  /** "attacker" / "defender" for the first-strike volley row, null otherwise. */
  first_strike_side: 'attacker' | 'defender' | null;
  atk_fp: number;
  def_fp: number;
  atk_shots: number;
  def_shots: number;
  atk_casualties: string[];
  def_casualties: string[];
  /** Set when this round triggered a mid-battle retreat. */
  retreat_triggered: 'attacker' | 'defender' | null;
}

/** Debug-only: explains the retreat decision for a battle. */
export interface RetreatDebug {
  /** "attacker", "defender", or "none" if no retreat fired. */
  side: 'attacker' | 'defender' | 'none';
  /** "pre_battle", "mid_battle", or "none". */
  stage: 'pre_battle' | 'mid_battle' | 'none';
  /** For pre-battle: opposing-FP / own-FP. For mid-battle: fraction of initial FP lost. */
  measured_value: number;
  /** Threshold the measured value was compared against. */
  threshold: number;
  attacker_prebattle_ratio: number;
  defender_prebattle_ratio: number;
  attacker_prebattle_threshold: number;
  defender_prebattle_threshold: number;
  /** Round of mid-battle retreat (1-based); 0 for pre-battle / no retreat. */
  round: number;
}

export interface LandBattleData {
  type: 'land';
  attacker: string;
  attacker_id: number;
  defender: string;
  defender_id: number;
  province: string;
  province_id: number;
  attacker_won: boolean;
  retreated: boolean;
  defender_retreated: boolean;
  attacker_casualties: string[];
  defender_casualties: string[];
  attacker_survivors: BattleUnit[];
  defender_survivors: BattleUnit[];
  terrain: string | null;
  fort_level: number;
  siege_reduced_fort: boolean;
  attacker_initial_count: number;
  defender_initial_count: number;
  attacker_initial_fp: number;
  defender_initial_fp: number;
  attacker_survivors_count: number;
  defender_survivors_count: number;
  medal_awards: MedalAward[];
  capital_tile: BattleTile | null;
  province_tiles: BattleTile[];
  origin_tiles: BattleTile[];
  origin_province_names: string[];
  is_naval_landing: boolean;
  retreat_debug: RetreatDebug | null;
  /** Per-unit logs captured at battle start, finalized post-resolution. */
  attacker_unit_logs: BattleUnitLog[];
  defender_unit_logs: BattleUnitLog[];
  /** Per-round trace; empty for short-circuited battles. */
  round_logs: BattleRoundLog[];
}

export interface NavalBattleData {
  type: 'naval';
  attacker: string;
  attacker_id: number;
  defender: string;
  defender_id: number;
  attacker_won: boolean;
  attacker_ships_lost: string[];
  defender_ships_lost: string[];
  attacker_survivors_count: number;
  defender_survivors_count: number;
}

export type BattleData = LandBattleData | NavalBattleData;

export interface ArchivedBattleTurn {
  turn: number;
  year: number;
  quarter: number;
  battles: LandBattleData[];
  naval_battles: NavalBattleData[];
}

export interface LedgerData {
  economy: {
    treasury: number;
    goods_revenue: number;
    subsidies: { nation: string; amount: number }[];
  };
  production: {
    buildings: { type: string; capacity: number; upgrading: boolean }[];
    resources: { name: string; quantity: number }[];
    materials: { name: string; quantity: number }[];
    goods: { name: string; quantity: number }[];
  };
  military: {
    army_by_type: { unit_type: string; count: number; firepower: number }[];
    total_army_fp: number;
    total_army_count: number;
    field_army_count: number;
    militia_count: number;
    warships_by_type: { ship_type: string; count: number }[];
    total_warship_count: number;
    merchant_ships: number;
    total_arms_built: number;
    generals_earned: number;
  };
  diplomacy: {
    standing: number;
    consulates: number;
    embassies: number;
    treaties: { nation: string; treaty_type: string }[];
    wars: string[];
  };
  labor: {
    untrained: number;
    trained: number;
    expert: number;
    total: number;
  };
}

export interface GPLedgerEntry {
  nation_id: number;
  nation_name: string;
  nation_color: string;
  is_human: boolean;
  economy: {
    treasury: number;
    provinces: number;
    buildings: number;
    goods_revenue: number;
    total_resources: number;
    total_materials: number;
    total_goods: number;
  };
  labor: {
    untrained: number;
    trained: number;
    expert: number;
    total: number;
  };
  military: {
    total_army_count: number;
    total_army_fp: number;
    field_army_count: number;
    militia_count: number;
    total_warship_count: number;
    merchant_ships: number;
    generals_earned: number;
    total_arms_built: number;
  };
  diplomacy: {
    standing: number;
    consulates: number;
    embassies: number;
    alliances: number;
    alliance_names: string[];
    wars: number;
    war_names: string[];
  };
  resources_detail: Record<string, number>;
  materials_detail: Record<string, number>;
  goods_detail: Record<string, number>;
  technology: {
    researched_count: number;
    researched_names: string[];
  };
  // Per-turn cash-flow breakdown from the last processed turn. Null before
  // the first turn has completed for the current game.
  cash_flow: CashFlowSnapshot | null;
  // Per-turn resource-flow breakdown from the last processed turn. Visibility
  // only — aggregated from existing TurnReport fields, NOT reconciled.
  resource_flow: ResourceFlowSnapshot | null;
  // Cumulative totals (dollars) keyed by enum variant name — e.g.
  // `"GoldGemsConversion": 25000`. Grown across every turn of the game.
  cumulative: {
    income_totals: Record<string, number>;
    expense_totals: Record<string, number>;
  };
}

export interface ResourceFlowEntry {
  stockpile: string;
  // Inflow entries have `source`; outflow entries have `sink`. Use whichever
  // is present.
  source?: string;
  sink?: string;
  // FlowCategory label: "Production" | "Trade" | "Consumption".
  category: string;
  amount: number;
}

export interface ResourceFlowSnapshot {
  inflow: ResourceFlowEntry[];
  outflow: ResourceFlowEntry[];
  // Per-stockpile, per-category totals. Shape:
  // { "Timber": { "Production": 10, "Trade": 5 }, ... }
  inflow_by_stockpile_category: Record<string, Record<string, number>>;
  outflow_by_stockpile_category: Record<string, Record<string, number>>;
}

export interface CashFlowSnapshot {
  opening_treasury: number;
  closing_treasury: number;
  total_income: number;
  total_expense: number;
  observed_delta: number;
  accounted_delta: number;
  reconciliation_mismatch: number;
  reconciles: boolean;
  // Keys are human-readable labels from `CashSource::label()` /
  // `CashSink::label()` in Rust, so CLI/batch/UI all agree.
  income_totals: Record<string, number>;
  expense_totals: Record<string, number>;
  // Bucketed totals by `FlowCategory` ("Production" / "Trade" /
  // "Consumption"). Lets the UI show a quick "money came from production
  // vs. trade" roll-up alongside the per-source detail.
  income_by_category: Record<string, number>;
  expense_by_category: Record<string, number>;
}

export interface DiplomacyOverlayRelation {
  nation_name: string;
  nation_id: number;
  nation_color: string;
  score: number;
  at_war: boolean;
  status: string;
  treaties: string[];
  has_consulate: boolean;
  has_embassy: boolean;
  has_pending_consulate: boolean;
  has_pending_embassy: boolean;
  has_pending_war: boolean;
  pending_grant_amount_dollars: number | null;
  pending_break_treaties: string[];
  has_pending_nap: boolean;
  has_pending_alliance: boolean;
  has_pending_peace: boolean;
}

export interface DiplomacyOverlay {
  selected_nation: string;
  selected_nation_id: number;
  relations: DiplomacyOverlayRelation[];
}

export interface MilitaryOverlayEntry {
  nation_name: string;
  nation_id: number;
  nation_color: string;
  total_army_fp: number;
  total_naval_fp: number;
  army_unit_count: number;
  warship_count: number;
}

// ── Unit detail types ────────────────────────────────────────────────

export interface ArmyUnitDetail {
  id: number;
  unit_type: string;
  category: string;
  owner_id: number;
  owner_name: string;
  health: number;
  medals: number;
  firepower: number;
  effective_firepower: number;
  movement: number;
  movement_remaining: number;
  /** Next-era variant the owner can upgrade to (tech-met), or null. */
  upgrade_to: string | null;
  /** Production-cost difference in dollars; null when upgrade_to is null. */
  upgrade_cost: number | null;
  /** Extra Arms required beyond the current variant; null when upgrade_to is null. */
  upgrade_arms_delta: number | null;
  /**
   * Debug-only: why this unit didn't heal last turn. One of "moved",
   * "fought", "full_health", or null when the unit healed normally.
   */
  heal_blocked_reason: 'moved' | 'fought' | 'full_health' | null;
}

export interface ProvinceUnits {
  army_units: ArmyUnitDetail[];
  garrison_count: number;
  province_name: string;
}

export interface CivilianDetail {
  id: number;
  type: string;
  position: { q: number; r: number } | null;
  working: boolean;
  turns_remaining: number;
  build_task?: string | null;
  tile_terrain?: string;
  tile_resource?: string | null;
}

export interface CiviliansData {
  deployed: CivilianDetail[];
  undeployed: CivilianDetail[];
}

export interface ShipDetail {
  id: number;
  type: string;
  hull: number;
  hull_max: number;
  firepower?: number;
  cargo?: number;
  sea_zone: number | null;
}

export interface ShipsData {
  merchants: ShipDetail[];
  warships: ShipDetail[];
  total_cargo: number;
  total_naval_fp: number;
}

export interface MoveTarget {
  province_id: number;
  name: string;
  owner?: string;
}

export interface ValidMoveTargets {
  friendly: MoveTarget[];
  hostile: MoveTarget[];
}

export interface BuildableUnit {
  type: string;
  category?: string;
  cost?: number;
  arms_required?: number;
  firepower?: number;
  movement?: number;
  hull?: number;
  cargo?: number;
  requires_horse?: boolean;
  resources_needed?: Record<string, number>;
  can_afford: boolean;
  tech_met: boolean;
  reason?: string | null;
  max_count?: number;
  expert_required?: boolean;
}

export interface BuildableUnits {
  army: BuildableUnit[];
  civilians: BuildableUnit[];
  ships: BuildableUnit[];
  treasury: number;
  arms: number;
}

export interface PendingMove {
  unit_id: number;
  destination_province_id: number;
  destination_name: string;
}

// Snapshot shape (post-refactor) places `nations`, `provinces`, `diplomacy`,
// `hex_map`, `market_state` under `world`, and `pending_moves`/`pending_attacks`
// under `transient`. Frontend code historically reads these as top-level
// fields, so we re-expose them here.
export function parseGameJson(json: string): any {
  const s: any = JSON.parse(json);
  if (s && typeof s === 'object' && !s.error) {
    if (s.world) {
      if (s.nations === undefined) s.nations = s.world.nations;
      if (s.provinces === undefined) s.provinces = s.world.provinces;
      if (s.diplomacy === undefined) s.diplomacy = s.world.diplomacy;
      if (s.hex_map === undefined) s.hex_map = s.world.hex_map;
      if (s.market_state === undefined) s.market_state = s.world.market_state;
      if (s.map_key === undefined) s.map_key = s.world.map_key;
    }
    // Hoist nation.archives display fields to the top level of each nation so
    // frontend code can access n.flag_svg, n.government_title etc. directly.
    if (Array.isArray(s.nations)) {
      for (const n of s.nations) {
        if (n.archives && typeof n.archives === 'object') {
          if (n.flag_svg === undefined) n.flag_svg = n.archives.flag_svg || '';
          if (n.government_title === undefined) n.government_title = n.archives.government_title || '';
          if (n.adjective === undefined) n.adjective = n.archives.adjective || '';
          if (n.demonym_singular === undefined) n.demonym_singular = n.archives.demonym_singular || '';
          if (n.demonym_plural === undefined) n.demonym_plural = n.archives.demonym_plural || '';
        }
      }
    }
    if (s.transient) {
      if (s.pending_moves === undefined) s.pending_moves = s.transient.pending_moves;
      if (s.pending_attacks === undefined) s.pending_attacks = s.transient.pending_attacks;
      if (s.pending_landings === undefined) s.pending_landings = s.transient.pending_landings;
    }
  }
  return s;
}

// ── Existing wrapper functions ───────────────────────────────────────

export interface TerrainMix {
  grassland: number;
  forest: number;
  hills: number;
  mountain: number;
  desert: number;
  swamp: number;
  tundra: number;
  forest_cluster: number;
  hills_cluster: number;
  mountain_cluster: number;
  desert_cluster: number;
  swamp_cluster: number;
  /** 0 = uniform tundra, 1 = strong concentration at top/bottom rows. */
  pole_tundra_strength: number;
  /** Outermost guaranteed-sea ring width (cells). */
  sea_hard_margin: number;
  /** Soft-falloff zone width (cells). Must exceed sea_hard_margin. */
  sea_falloff_radius: number;
  /** Multiplier on continent target size: 1 = baseline, 0.3 = sparse, 2 = dense. */
  land_amount: number;
}

export const DEFAULT_TERRAIN_MIX: TerrainMix = {
  grassland: 55,
  forest: 20,
  hills: 13,
  mountain: 5,
  desert: 3,
  swamp: 3,
  tundra: 1,
  forest_cluster: 25,
  hills_cluster: 20,
  mountain_cluster: 12,
  desert_cluster: 15,
  swamp_cluster: 10,
  pole_tundra_strength: 0.5,
  sea_hard_margin: 1,
  sea_falloff_radius: 5,
  land_amount: 1.0,
};

export interface MapGenConfig {
  width: number;
  height: number;
  numGreatPowers: number;
  numMinorNations: number;
  terrain?: TerrainMix;
}

export const DEFAULT_MAP_GEN_CONFIG: MapGenConfig = {
  width: 80,
  height: 50,
  numGreatPowers: 7,
  numMinorNations: 16,
  terrain: DEFAULT_TERRAIN_MIX,
};

function terrainJson(cfg: MapGenConfig): string {
  return cfg.terrain ? JSON.stringify(cfg.terrain) : '';
}

export async function newGame(
  mapKey: string,
  difficulty: number,
  nationIndex: number,
  cfg: MapGenConfig = DEFAULT_MAP_GEN_CONFIG,
  flavorKey: string = '',
): Promise<string> {
  return call<string>(
    'wasm_new_game',
    mapKey,
    difficulty,
    nationIndex,
    cfg.width,
    cfg.height,
    cfg.numGreatPowers,
    cfg.numMinorNations,
    flavorKey,
    terrainJson(cfg),
  );
}

export async function processTurn(gameJson: string): Promise<any> {
  const result = await call<string>('wasm_process_turn', gameJson);
  return JSON.parse(result);
}

export async function getMapData(gameJson: string, disableFog: boolean = false): Promise<TileData[]> {
  return JSON.parse(await call<string>('wasm_get_map_data', gameJson, disableFog));
}

export async function getNavyMarkers(gameJson: string, disableFog: boolean = false): Promise<NavyMarker[]> {
  return JSON.parse(await call<string>('wasm_get_navy_markers', gameJson, disableFog));
}

export async function getSeaZones(gameJson: string): Promise<SeaZone[]> {
  return JSON.parse(await call<string>('wasm_get_sea_zones', gameJson));
}

export interface TechEntry {
  id: number;
  name: string;
  cost: number;
  earliest_year?: number;
  latest_year?: number;
  description?: string;
}

export interface ResearchedTechEntry {
  id: number;
  name: string;
  year: number;
  description?: string;
}

export interface TechScreenData {
  available: TechEntry[];
  researched: ResearchedTechEntry[];
  pending: TechEntry | null;
  treasury: number;
}

export async function getTechScreenData(gameJson: string): Promise<TechScreenData | null> {
  const data = JSON.parse(await call<string>('wasm_get_tech_screen_data', gameJson));
  if (data && typeof data === 'object' && 'error' in data) return null;
  return data as TechScreenData;
}

export async function queueTechResearch(gameJson: string, techName: string): Promise<{ ok: boolean; gameJson?: string; error?: string }> {
  const result = await call<string>('wasm_queue_tech_research', gameJson, techName);
  try {
    const parsed = JSON.parse(result);
    if (parsed.error) return { ok: false, error: parsed.error };
    return { ok: true, gameJson: result };
  } catch {
    return { ok: true, gameJson: result };
  }
}

export async function cancelTechResearch(gameJson: string): Promise<{ ok: boolean; gameJson?: string; error?: string }> {
  const result = await call<string>('wasm_cancel_tech_research', gameJson);
  try {
    const parsed = JSON.parse(result);
    if (parsed.error) return { ok: false, error: parsed.error };
    return { ok: true, gameJson: result };
  } catch {
    return { ok: true, gameJson: result };
  }
}

export async function getScenarios(): Promise<any[]> {
  return JSON.parse(await call<string>('wasm_get_scenarios'));
}

export async function newScenarioGame(
  scenarioId: string,
  difficulty: number,
  nationIndex: number,
  flavorKey: string = '',
): Promise<string> {
  return call<string>('wasm_new_scenario_game', scenarioId, difficulty, nationIndex, flavorKey);
}

export async function newObserverGame(
  mapKey: string,
  difficulty: number,
  cfg: MapGenConfig = DEFAULT_MAP_GEN_CONFIG,
  flavorKey: string = '',
): Promise<string> {
  return call<string>(
    'wasm_new_observer_game',
    mapKey,
    difficulty,
    cfg.width,
    cfg.height,
    cfg.numGreatPowers,
    cfg.numMinorNations,
    flavorKey,
    terrainJson(cfg),
  );
}

export async function newObserverScenarioGame(
  scenarioId: string,
  difficulty: number,
  flavorKey: string = '',
): Promise<string> {
  return call<string>('wasm_new_observer_scenario_game', scenarioId, difficulty, flavorKey);
}

/// Re-roll names/flags/government titles on an existing game state.
/// Map layout, ownership and any other state is preserved.
export async function applyFlavor(gameJson: string, flavorKey: string): Promise<string> {
  return call<string>('wasm_apply_flavor', gameJson, flavorKey);
}

export async function setHumanPlayer(gameJson: string, nationIndex: number): Promise<string> {
  return call<string>('wasm_set_human_player', gameJson, nationIndex);
}

export interface BulkTurnReport {
  turn: string;
  year: number;
  quarter: number;
  headlines: Headline[];
  battles: LandBattleData[];
  naval_battles: NavalBattleData[];
  scores: Record<string, number>;
}

export interface BulkTurnResult {
  game: any;
  reports: BulkTurnReport[];
  stopped_early: boolean;
}

export async function processTurns(gameJson: string, count: number): Promise<BulkTurnResult> {
  // Each turn includes full AI/economy resolution; a batch of up to 50 turns
  // easily exceeds the default 30s call timeout, so allow up to 5 minutes.
  return JSON.parse(await callWithTimeout<string>('wasm_process_turns', 300_000, gameJson, count));
}

export async function getDiplomacyOverlay(gameJson: string, nationId: number): Promise<DiplomacyOverlay | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_diplomacy_overlay', gameJson, nationId));
  if (parsed.error || !parsed.relations) return null;
  return parsed;
}

export async function getNewspaperArchive(gameJson: string): Promise<ArchivedNewspaper[]> {
  const parsed = JSON.parse(await call<string>('wasm_get_newspaper_archive', gameJson));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export async function getNewspaperArchiveSince(gameJson: string, afterTurn: number): Promise<ArchivedNewspaper[]> {
  const parsed = JSON.parse(await call<string>('wasm_get_newspaper_archive_since', gameJson, afterTurn));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export async function getPoliticalSnapshot(gameJson: string, turn: number): Promise<PoliticalSnapshot | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_political_snapshot', gameJson, turn));
  if (parsed.error) return null;
  return parsed;
}

export async function getBattleArchive(gameJson: string): Promise<ArchivedBattleTurn[]> {
  const parsed = JSON.parse(await call<string>('wasm_get_battle_data', gameJson));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export async function getAllGPLedgerData(gameJson: string): Promise<GPLedgerEntry[]> {
  const parsed = JSON.parse(await call<string>('wasm_get_all_gp_ledger_data', gameJson));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export async function getLedgerData(gameJson: string, nationId: number): Promise<LedgerData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_ledger_data', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function getMilitaryOverlay(gameJson: string): Promise<MilitaryOverlayEntry[]> {
  const parsed = JSON.parse(await call<string>('wasm_get_military_overlay', gameJson));
  if (!Array.isArray(parsed)) return [];
  return parsed;
}

// ── New query functions ──────────────────────────────────────────────

export async function getUnitsInProvince(gameJson: string, provinceId: number): Promise<ProvinceUnits | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_units_in_province', gameJson, provinceId));
  if (parsed.error) return null;
  return parsed;
}

export async function getCivilians(gameJson: string, nationId: number): Promise<CiviliansData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_civilians', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function getShips(gameJson: string, nationId: number): Promise<ShipsData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_ships', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function getValidMoveTargets(gameJson: string, nationId: number, unitId: number): Promise<ValidMoveTargets | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_valid_move_targets', gameJson, nationId, unitId));
  if (parsed.error) return null;
  return parsed;
}

export async function getBuildableUnits(gameJson: string, nationId: number): Promise<BuildableUnits | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_buildable_units', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

// ── New command functions ────────────────────────────────────────────

export interface CommandResult {
  ok: boolean;
  gameJson?: string;
  error?: string;
}

function executeCommand(result: string): CommandResult {
  if (result.startsWith('{"error"')) {
    try {
      const parsed = JSON.parse(result);
      return { ok: false, error: parsed.error || 'Unknown error' };
    } catch {
      return { ok: false, error: result };
    }
  }
  return { ok: true, gameJson: result };
}

async function runCmd(fn: string, ...args: any[]): Promise<CommandResult> {
  return executeCommand(await call<string>(fn, ...args));
}

export async function queueUnitMove(gameJson: string, nationId: number, unitId: number, destProvinceId: number): Promise<CommandResult> {
  return runCmd('wasm_queue_unit_move', gameJson, nationId, unitId, destProvinceId);
}

export async function cancelUnitMove(gameJson: string, unitId: number): Promise<CommandResult> {
  return runCmd('wasm_cancel_unit_move', gameJson, unitId);
}

export async function disbandUnit(gameJson: string, unitId: number): Promise<CommandResult> {
  return runCmd('wasm_disband_unit', gameJson, unitId);
}

export async function deployCivilian(gameJson: string, civilianId: number, q: number, r: number): Promise<CommandResult> {
  return runCmd('wasm_deploy_civilian', gameJson, civilianId, q, r);
}

export async function recallCivilian(gameJson: string, civilianId: number): Promise<CommandResult> {
  return runCmd('wasm_recall_civilian', gameJson, civilianId);
}

export type EngineerBuildKind = 'railroad' | 'depot' | 'port';

export async function engineerBuild(gameJson: string, civilianId: number, kind: EngineerBuildKind): Promise<CommandResult> {
  return runCmd('wasm_engineer_build', gameJson, civilianId, kind);
}

export async function recruitArmyUnit(gameJson: string, nationId: number, unitType: string): Promise<CommandResult> {
  return runCmd('wasm_recruit_army_unit', gameJson, nationId, unitType);
}

// ── Naval movement (Card #471) ───────────────────────────────────────

/**
 * Queue a fleet move from `fromZoneId` to the adjacent `toZoneId` (card
 * #471). The two zones must share a sea-zone adjacency edge. Resolution
 * happens at end-of-turn via the domain processor — same shape as army
 * `pending_moves`. Re-queueing from the same source zone replaces the
 * previous entry.
 */
export async function moveFleet(
  gameJson: string,
  nationId: number,
  fromZoneId: number,
  toZoneId: number,
): Promise<CommandResult> {
  return runCmd('wasm_move_fleet', gameJson, nationId, fromZoneId, toZoneId);
}

/**
 * Cancel a queued fleet move from `fromZoneId`. No-op if no such pending
 * move exists.
 */
export async function cancelFleetMove(
  gameJson: string,
  nationId: number,
  fromZoneId: number,
): Promise<CommandResult> {
  return runCmd('wasm_cancel_fleet_move', gameJson, nationId, fromZoneId);
}

// ── Unit upgrades (Card #417) ────────────────────────────────────────

export async function upgradeUnit(gameJson: string, nationId: number, unitId: number): Promise<CommandResult> {
  return runCmd('wasm_upgrade_unit', gameJson, nationId, unitId);
}

export interface BulkUpgradeFailure {
  unit_id: number;
  error: string;
}

/**
 * Discriminated outcome of `upgradeUnits`. Either a top-level command
 * error (unparsable input, snapshot decode failure, etc.) or the bulk
 * result with per-unit failures embedded.
 */
export type BulkUpgradeResult =
  | { kind: 'error'; error: string }
  | { kind: 'ok'; upgraded: number; failed: BulkUpgradeFailure[]; gameJson: string };

/**
 * Upgrade many units in a single call. Per-unit failures don't abort the
 * batch — they show up in `failed[]`. Top-level wasm errors (e.g. bad
 * JSON, unknown nation) are surfaced as `{ kind: 'error' }` so the caller
 * never tries to apply an undefined game state.
 */
export async function upgradeUnits(gameJson: string, nationId: number, unitIds: number[]): Promise<BulkUpgradeResult> {
  const raw = await call<string>('wasm_upgrade_units', gameJson, nationId, JSON.stringify(unitIds));
  let parsed: any;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    return { kind: 'error', error: `bad bulk-upgrade JSON: ${(e as Error).message}` };
  }
  if (parsed && typeof parsed === 'object' && typeof parsed.error === 'string') {
    return { kind: 'error', error: parsed.error };
  }
  if (!parsed || typeof parsed.upgraded !== 'number' || !parsed.game) {
    return { kind: 'error', error: 'bulk-upgrade result missing game state' };
  }
  return {
    kind: 'ok',
    upgraded: parsed.upgraded,
    failed: Array.isArray(parsed.failed) ? parsed.failed : [],
    gameJson: JSON.stringify(parsed.game),
  };
}

export interface UpgradeInfo {
  upgrade_to: string | null;
  cost?: number;
  arms_delta?: number;
  tech_met?: boolean;
}

export async function getUpgradeInfo(gameJson: string, nationId: number, unitId: number): Promise<UpgradeInfo> {
  const raw = await call<string>('wasm_get_upgrade_info', gameJson, nationId, unitId);
  return JSON.parse(raw) as UpgradeInfo;
}

export async function setPendingCivilianHire(gameJson: string, nationId: number, civilianType: string, count: number): Promise<CommandResult> {
  return runCmd('wasm_set_pending_civilian_hire', gameJson, nationId, civilianType, count);
}

export async function buildShip(gameJson: string, nationId: number, shipType: string): Promise<CommandResult> {
  return runCmd('wasm_build_ship', gameJson, nationId, shipType);
}

export async function cancelShipBuild(gameJson: string, nationId: number, shipType: string): Promise<CommandResult> {
  return runCmd('wasm_cancel_ship_build', gameJson, nationId, shipType);
}

export async function setPendingShips(gameJson: string, nationId: number, shipType: string, count: number): Promise<CommandResult> {
  return runCmd('wasm_set_pending_ships', gameJson, nationId, shipType, Math.max(0, Math.round(count)));
}

export async function setAutoTradeWithMinors(gameJson: string, nationId: number, enabled: boolean): Promise<CommandResult> {
  return runCmd('wasm_set_auto_trade_with_minors', gameJson, nationId, enabled);
}

export async function setPendingArmyRecruit(gameJson: string, nationId: number, unitType: string, count: number): Promise<CommandResult> {
  return runCmd('wasm_set_pending_army_recruits', gameJson, nationId, unitType, Math.max(0, Math.round(count)));
}

// ── Transport types & functions ─────────────────────────────────────

export interface TransportAllocation {
  resource: string;
  units: number;
}

export interface TransportDelivery {
  resource: string;
  available: number;
  delivered: number;
}

export interface TransportDemand {
  resource: string;
  demand: number;
}

export interface TransportData {
  freight_cars: number;
  total_capacity: number;
  remote_delivery_capacity: number;
  military_transport_capacity: number;
  allocations: TransportAllocation[];
  build_cost: { labor: number; lumber: number; steel: number };
  can_build: boolean;
  available_lumber: number;
  available_steel: number;
  available_labor: number;
  deliveries: TransportDelivery[];
  local_deliveries?: TransportDelivery[];
  demand: TransportDemand[];
}

export async function getTransportData(gameJson: string, nationId: number): Promise<TransportData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_transport_data', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function setPendingFreightCars(gameJson: string, nationId: number, count: number): Promise<CommandResult> {
  return runCmd('wasm_set_pending_freight_cars', gameJson, nationId, Math.max(0, Math.round(count)));
}

export async function setTransportAllocation(gameJson: string, nationId: number, resource: string, units: number): Promise<CommandResult> {
  return runCmd('wasm_set_transport_allocation', gameJson, nationId, resource, Math.max(0, Math.round(units)));
}

// ── Industry types & functions ──────────────────────────────────────

export interface BuildingInfo {
  type: string;
  display_name: string;
  capacity: number;
  next_capacity: number;
  is_expanding: boolean;
  turns_remaining: number;
  pending_capacity: number;
  expansion_cost: { lumber: number; steel: number };
}

/** u32::MAX sentinel means "unlimited" — slider at maximum position */
export const CHAIN_TARGET_UNLIMITED = 4294967295;

export interface ChainForecast {
  mill_target: number;
  mill_cap: number;
  mill_output: number;
  mill_labor: number;
  mill_max_output: number;
  // committed resource inputs (capped by target)
  mill_committed_timber?: number;
  mill_committed_coal?: number;
  mill_committed_iron?: number;
  mill_committed_cotton?: number;
  mill_committed_wool?: number;
  factory_target: number;
  factory_cap: number;
  factory_output: number;
  factory_labor: number;
  factory_max_output: number;
  factory_committed_lumber?: number;
  factory_committed_steel?: number;
  factory_committed_fabric?: number;
}

export interface ChainOutputTargets {
  timber_mill: number;
  metal_mill: number;
  textile_mill: number;
  lumber_factory: number;
  steel_factory: number;
  garment_factory: number;
  armory: number;
  paper_factory: number;
  canned_food_factory: number;
}

export interface PaperChainForecast {
  factory_cap: number;
  factory_target: number;
  factory_output: number;
  factory_labor: number;
  factory_max_output: number;
  factory_committed_lumber: number;
}

export interface FoodChainForecast {
  factory_cap: number;
  factory_target: number;
  factory_output: number;
  factory_labor: number;
  factory_max_output: number;
  factory_committed_grain: number;
  factory_committed_fruit: number;
  factory_committed_fish: number;
  factory_committed_livestock: number;
}

export interface ArmsChainForecast {
  armory_cap: number;
  armory_target: number;
  armory_output: number;
  armory_labor: number;
  armory_max_output: number;
  armory_committed_steel: number;
}

export interface TrainingCosts {
  to_trained_paper: number;
  to_trained_labor: number;
  to_expert_paper: number;
  to_expert_labor: number;
}

export interface ImmigrationCosts {
  canned_food: number;
  clothing: number;
}

export interface IndustryData {
  buildings: BuildingInfo[];
  freight_car_cost: number;
  pending_freight_cars: number;
  max_freight_cars: number;
  warehouse: {
    resources: Record<string, number>;
    materials: Record<string, number>;
    goods: Record<string, number>;
  };
  /**
   * Per-commodity stock targets the AI is aiming to maintain. Resources use
   * the buy-side stockpile target (per-turn demand × buffer turns); materials
   * and goods use the sell-side reserve held back from minor-nation trade.
   */
  warehouse_targets: {
    resources: Record<string, number>;
    materials: Record<string, number>;
    goods: Record<string, number>;
  };
  labor: {
    untrained: number;
    trained: number;
    expert: number;
    total_workers: number;
    total_labor_units: number;
    committed_expert: number;
    committed_untrained: number;
    committed_trained: number;
    committed_labor_units: number;
  };
  chain_targets: ChainOutputTargets;
  production_forecast: {
    timber_chain: ChainForecast;
    metal_chain: ChainForecast;
    textile_chain: ChainForecast;
    arms_chain: ArmsChainForecast;
    paper_chain: PaperChainForecast;
    food_chain: FoodChainForecast;
  };
  pending_ships: string[];
  pending_army_recruits: string[];
  army_committed_arms: number;
  army_committed_horses: number;
  auto_trade_with_minors: boolean;
  can_expand: Record<string, boolean>;
  pending_civilian_hires: Record<string, number>;
  pending_immigration: number;
  max_pending_immigration: number;
  pending_training: { to_trained: number; to_expert: number };
  immigration_costs: ImmigrationCosts;
  training_costs: TrainingCosts;
}

export async function getIndustryData(gameJson: string, nationId: number): Promise<IndustryData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_industry_data', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function expandBuilding(gameJson: string, nationId: number, buildingType: string): Promise<CommandResult> {
  return runCmd('wasm_expand_building', gameJson, nationId, buildingType);
}

export async function setChainTarget(gameJson: string, nationId: number, chain: string, step: string, target: number): Promise<CommandResult> {
  const safeTarget = Math.max(0, Math.round(target));
  return runCmd('wasm_set_chain_target', gameJson, nationId, chain, step, safeTarget);
}

export async function setPendingTraining(gameJson: string, nationId: number, toTrained: number, toExpert: number): Promise<CommandResult> {
  return runCmd('wasm_set_pending_training', gameJson, nationId, Math.max(0, Math.round(toTrained)), Math.max(0, Math.round(toExpert)));
}

export async function setPendingImmigration(gameJson: string, nationId: number, count: number): Promise<CommandResult> {
  return runCmd('wasm_set_pending_immigration', gameJson, nationId, Math.max(0, Math.round(count)));
}

// ── Trade types & functions ─────────────────────────────────────────

export interface MarketPrice {
  resource: string;
  base_price: number;
  stock: number;
}

export interface TradeHistoryItem {
  turn: number;
  partner_name: string;
  partner_id: number;
  resource: string;
  quantity: number;
  total_cost: number;
  bought: boolean;
}

export interface TradeSubsidy {
  nation_id: number;
  nation_name: string;
  amount: number;
  has_consulate: boolean;
}

export interface MinorNationTrade {
  nation_id: number;
  name: string;
  has_consulate: boolean;
  has_embassy: boolean;
  resources: string[];
}

export interface PlayerSellOrder {
  commodity_type: string;
  commodity_name: string;
  quantity: number;
  price: number;
}

export interface PlayerBuyOrder {
  commodity_type: string;
  commodity_name: string;
  resource: string; // alias of commodity_name for backwards-compat
  quantity: number;
  max_price: number;
}

export interface AvailableOffer {
  seller_id: number;
  seller_name: string;
  resource: string;
  quantity: number;
  price: number;
  is_great_power: boolean;
}

export interface SellableItem {
  name: string;
  stock: number;
  price: number;
}

export interface TradeData {
  market_prices: MarketPrice[];
  trade_history: TradeHistoryItem[];
  subsidies: TradeSubsidy[];
  trade_balance: { total_bought: number; total_sold: number; net: number };
  total_cargo: number;
  remaining_cargo: number;
  minor_nations: MinorNationTrade[];
  treasury: number;
  player_sell_orders: PlayerSellOrder[];
  player_buy_orders: PlayerBuyOrder[];
  available_offers: AvailableOffer[];
  sellable_resources: SellableItem[];
  auto_trade_with_minors: boolean;
}

export async function getTradeData(gameJson: string, nationId: number): Promise<TradeData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_trade_data', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function setTradeSubsidy(gameJson: string, nationId: number, targetNationId: number, amount: number): Promise<CommandResult> {
  return runCmd('wasm_set_trade_subsidy', gameJson, nationId, targetNationId, BigInt(amount));
}

export async function setPlayerSellOrder(gameJson: string, nationId: number, commodityType: string, commodityName: string, quantity: number): Promise<CommandResult> {
  return runCmd('wasm_set_player_sell_order', gameJson, nationId, commodityType, commodityName, quantity);
}

export async function setPlayerBuyOrder(gameJson: string, nationId: number, commodityType: string, commodityName: string, quantity: number, maxPrice: number): Promise<CommandResult> {
  return runCmd('wasm_set_player_buy_order', gameJson, nationId, commodityType, commodityName, quantity, BigInt(maxPrice));
}

// ── Diplomacy Screen types & functions ──────────────────────────────

export interface DiplomacyRelationActions {
  can_build_consulate: boolean;
  consulate_cost: number;
  can_build_embassy: boolean;
  embassy_cost: number;
  can_propose_nap: boolean;
  can_propose_alliance: boolean;
  can_declare_war: boolean;
  can_send_grant: boolean;
  can_break_treaty: boolean;
  breakable_treaties: string[];
  can_propose_peace: boolean;
}

export interface DiplomacyScreenRelation {
  nation_id: number;
  nation_name: string;
  nation_color: string;
  nation_type: string;
  score: number;
  at_war: boolean;
  status: string;
  treaties: string[];
  has_consulate: boolean;
  has_embassy: boolean;
  has_pending_consulate: boolean;
  has_pending_embassy: boolean;
  has_pending_war: boolean;
  pending_grant_amount_dollars: number | null;
  pending_break_treaties: string[];
  has_pending_nap: boolean;
  has_pending_alliance: boolean;
  has_pending_peace: boolean;
  is_in_anarchy: boolean;
  actions: DiplomacyRelationActions;
}

export interface DiplomacyScreenData {
  player_standing: number;
  treasury: number;
  relations: DiplomacyScreenRelation[];
}

export async function getDiplomacyScreenData(gameJson: string, nationId: number): Promise<DiplomacyScreenData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_diplomacy_screen_data', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function diplomacyBuildConsulate(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_build_consulate', gameJson, nationId, targetId);
}

export async function diplomacyBuildEmbassy(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_build_embassy', gameJson, nationId, targetId);
}

export async function diplomacyProposeNap(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_propose_nap', gameJson, nationId, targetId);
}

export async function diplomacyProposeAlliance(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_propose_alliance', gameJson, nationId, targetId);
}

export async function diplomacyDeclareWar(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_declare_war', gameJson, nationId, targetId);
}

export async function diplomacySendGrant(gameJson: string, nationId: number, targetId: number, amount: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_send_grant', gameJson, nationId, targetId, BigInt(amount));
}

export async function diplomacyBreakTreaty(gameJson: string, nationId: number, targetId: number, treatyType: string): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_break_treaty', gameJson, nationId, targetId, treatyType);
}

export async function diplomacyProposePeace(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_propose_peace', gameJson, nationId, targetId);
}

export async function diplomacyDismissOutgoingProposal(gameJson: string, nationId: number, targetId: number): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_dismiss_outgoing_proposal', gameJson, nationId, targetId);
}

export async function diplomacyDismissPendingAction(gameJson: string, nationId: number, targetId: number, actionKey: string): Promise<CommandResult> {
  return runCmd('wasm_diplomacy_dismiss_pending_action', gameJson, nationId, targetId, actionKey);
}

// ── Proposal Modal types & functions ────────────────────────────────

export interface PendingProposal {
  index: number;
  from_nation_id: number;
  from_nation_name: string;
  from_nation_color: string;
  proposal_type: string;
  display_text: string;
  turn_proposed: number;
  turns_until_expiry: number;
}

export interface ProposalData {
  proposals: PendingProposal[];
}

export async function getPendingProposals(gameJson: string, nationId: number): Promise<ProposalData | null> {
  const parsed = JSON.parse(await call<string>('wasm_get_pending_proposals', gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export async function acceptProposal(gameJson: string, nationId: number, proposalIndex: number): Promise<CommandResult> {
  return runCmd('wasm_accept_proposal', gameJson, nationId, proposalIndex);
}

export async function rejectProposal(gameJson: string, nationId: number, proposalIndex: number): Promise<CommandResult> {
  return runCmd('wasm_reject_proposal', gameJson, nationId, proposalIndex);
}
