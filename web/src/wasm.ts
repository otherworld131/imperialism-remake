import init, {
  wasm_new_game,
  wasm_new_scenario_game,
  wasm_process_turn,
  wasm_get_map_data,
  wasm_get_available_techs,
  wasm_research_tech,
  wasm_get_scenarios,
  wasm_get_diplomacy_overlay,
  wasm_get_military_overlay,
  wasm_get_units_in_province,
  wasm_get_civilians,
  wasm_get_ships,
  wasm_get_valid_move_targets,
  wasm_get_buildable_units,
  wasm_queue_unit_move,
  wasm_cancel_unit_move,
  wasm_deploy_civilian,
  wasm_recall_civilian,
  wasm_recruit_army_unit,
  wasm_hire_civilian,
  wasm_build_ship,
} from '../../crates/wasm-bridge/pkg/wasm_bridge.js';

let initialized = false;

export async function initWasm() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

export type MapMode = 'terrain' | 'political' | 'diplomatic' | 'relationship' | 'military' | 'naval';

export interface TileData {
  q: number; r: number;
  terrain: string; resource: string | null; resource_hidden: boolean;
  is_capital: boolean;
  is_country_capital: boolean;
  improvement_level: number;
  owner: string; owner_color: string; province: string;
  province_id: number | null;
  has_railroad: boolean; has_depot: boolean; has_port: boolean;
  has_fort: boolean; fort_level: number;
  map_width: number;
  nation_id: number;
  army_firepower: number;
  army_unit_count: number;
  naval_firepower: number;
  naval_ship_count: number;
  civilian_on_tile: { id: number; type: string; working: boolean; turns_remaining: number } | null;
}

export interface Headline {
  text: string;
  category: 'war' | 'battle' | 'diplomacy' | 'growth' | 'trade' | 'crisis' | 'politics' | 'military' | 'default';
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

// ── Existing wrapper functions ───────────────────────────────────────

export function newGame(mapKey: string, difficulty: number, nationIndex: number): string {
  return wasm_new_game(mapKey, difficulty, nationIndex);
}

export function processTurn(gameJson: string): any {
  const result = wasm_process_turn(gameJson);
  return JSON.parse(result);
}

export function getMapData(gameJson: string): TileData[] {
  return JSON.parse(wasm_get_map_data(gameJson));
}

export function getAvailableTechs(gameJson: string): any[] {
  return JSON.parse(wasm_get_available_techs(gameJson));
}

export function researchTech(gameJson: string, techName: string): string {
  return wasm_research_tech(gameJson, techName);
}

export function getScenarios(): any[] {
  return JSON.parse(wasm_get_scenarios());
}

export function newScenarioGame(scenarioId: string, difficulty: number, nationIndex: number): string {
  return wasm_new_scenario_game(scenarioId, difficulty, nationIndex);
}

export function getDiplomacyOverlay(gameJson: string, nationId: number): DiplomacyOverlay | null {
  const parsed = JSON.parse(wasm_get_diplomacy_overlay(gameJson, nationId));
  if (parsed.error || !parsed.relations) return null;
  return parsed;
}

export function getMilitaryOverlay(gameJson: string): MilitaryOverlayEntry[] {
  const parsed = JSON.parse(wasm_get_military_overlay(gameJson));
  if (!Array.isArray(parsed)) return [];
  return parsed;
}

// ── New query functions ──────────────────────────────────────────────

export function getUnitsInProvince(gameJson: string, provinceId: number): ProvinceUnits | null {
  const parsed = JSON.parse(wasm_get_units_in_province(gameJson, provinceId));
  if (parsed.error) return null;
  return parsed;
}

export function getCivilians(gameJson: string, nationId: number): CiviliansData | null {
  const parsed = JSON.parse(wasm_get_civilians(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function getShips(gameJson: string, nationId: number): ShipsData | null {
  const parsed = JSON.parse(wasm_get_ships(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function getValidMoveTargets(gameJson: string, nationId: number, unitId: number): ValidMoveTargets | null {
  const parsed = JSON.parse(wasm_get_valid_move_targets(gameJson, nationId, unitId));
  if (parsed.error) return null;
  return parsed;
}

export function getBuildableUnits(gameJson: string, nationId: number): BuildableUnits | null {
  const parsed = JSON.parse(wasm_get_buildable_units(gameJson, nationId));
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

export function queueUnitMove(gameJson: string, nationId: number, unitId: number, destProvinceId: number): CommandResult {
  return executeCommand(wasm_queue_unit_move(gameJson, nationId, unitId, destProvinceId));
}

export function cancelUnitMove(gameJson: string, unitId: number): CommandResult {
  return executeCommand(wasm_cancel_unit_move(gameJson, unitId));
}

export function deployCivilian(gameJson: string, civilianId: number, q: number, r: number): CommandResult {
  return executeCommand(wasm_deploy_civilian(gameJson, civilianId, q, r));
}

export function recallCivilian(gameJson: string, civilianId: number): CommandResult {
  return executeCommand(wasm_recall_civilian(gameJson, civilianId));
}

export function recruitArmyUnit(gameJson: string, nationId: number, unitType: string): CommandResult {
  return executeCommand(wasm_recruit_army_unit(gameJson, nationId, unitType));
}

export function hireCivilian(gameJson: string, nationId: number, civilianType: string): CommandResult {
  return executeCommand(wasm_hire_civilian(gameJson, nationId, civilianType));
}

export function buildShip(gameJson: string, nationId: number, shipType: string): CommandResult {
  return executeCommand(wasm_build_ship(gameJson, nationId, shipType));
}
