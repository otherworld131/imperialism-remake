import init, {
  wasm_new_game,
  wasm_new_scenario_game,
  wasm_new_observer_game,
  wasm_new_observer_scenario_game,
  wasm_set_human_player,
  wasm_process_turns,
  wasm_process_turn,
  wasm_get_map_data,
  wasm_get_navy_markers,
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
  wasm_engineer_build,
  wasm_recruit_army_unit,
  wasm_hire_civilian,
  wasm_build_ship,
  // Transport
  wasm_get_transport_data,
  wasm_build_freight_car,
  wasm_set_transport_allocation,
  // Industry
  wasm_get_industry_data,
  wasm_expand_building,
  // Trade
  wasm_get_trade_data,
  wasm_set_trade_subsidy,
  wasm_set_player_sell_order,
  wasm_set_player_buy_order,
  // Diplomacy
  wasm_get_diplomacy_screen_data,
  wasm_diplomacy_build_consulate,
  wasm_diplomacy_build_embassy,
  wasm_diplomacy_propose_nap,
  wasm_diplomacy_propose_alliance,
  wasm_diplomacy_declare_war,
  wasm_diplomacy_send_grant,
  wasm_diplomacy_break_treaty,
  wasm_diplomacy_propose_peace,
  // Proposals
  wasm_get_pending_proposals,
  wasm_accept_proposal,
  wasm_reject_proposal,
  // Newspaper archive
  wasm_get_newspaper_archive,
  wasm_get_political_snapshot,
  // Battle archive
  wasm_get_battle_data,
  // Ledger
  wasm_get_ledger_data,
  wasm_get_all_gp_ledger_data,
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
  max_improvement_level: number;
  owner: string; owner_color: string; province: string;
  province_id: number | null;
  has_railroad: boolean; has_depot: boolean; has_port: boolean;
  has_fort: boolean; fort_level: number;
  map_width: number;
  nation_id: number;
  army_firepower: number;
  army_unit_count: number;
  army_composition: Record<string, number> | null;
  naval_firepower: number;
  naval_ship_count: number;
  civilian_on_tile: { id: number; type: string; working: boolean; turns_remaining: number; build_task: string | null; owner: string; owner_color: string; is_human: boolean } | null;
  is_minor: boolean;
  is_incorporated_minor: boolean;
  is_anarchic: boolean;
  visual_group: string | null;
  visible: boolean;
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
}

export interface Headline {
  text: string;
  category: 'war' | 'battle' | 'diplomacy' | 'growth' | 'trade' | 'crisis' | 'politics' | 'military' | 'default';
  reason?: string;
  is_non_action?: boolean;
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
  attacker_casualties: string[];
  defender_casualties: string[];
  attacker_survivors: BattleUnit[];
  defender_survivors: BattleUnit[];
  terrain: string | null;
  fort_level: number;
  siege_reduced_fort: boolean;
  attacker_initial_count: number;
  defender_initial_count: number;
  attacker_survivors_count: number;
  defender_survivors_count: number;
  medal_awards: MedalAward[];
  capital_tile: BattleTile | null;
  province_tiles: BattleTile[];
  origin_tiles: BattleTile[];
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

export function getMapData(gameJson: string, disableFog: boolean = false): TileData[] {
  return JSON.parse(wasm_get_map_data(gameJson, disableFog));
}

export function getNavyMarkers(gameJson: string, disableFog: boolean = false): NavyMarker[] {
  return JSON.parse(wasm_get_navy_markers(gameJson, disableFog));
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

export function newObserverGame(mapKey: string, difficulty: number): string {
  return wasm_new_observer_game(mapKey, difficulty);
}

export function newObserverScenarioGame(scenarioId: string, difficulty: number): string {
  return wasm_new_observer_scenario_game(scenarioId, difficulty);
}

export function setHumanPlayer(gameJson: string, nationIndex: number): string {
  return wasm_set_human_player(gameJson, nationIndex);
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

export function processTurns(gameJson: string, count: number): BulkTurnResult {
  return JSON.parse(wasm_process_turns(gameJson, count));
}

export function getDiplomacyOverlay(gameJson: string, nationId: number): DiplomacyOverlay | null {
  const parsed = JSON.parse(wasm_get_diplomacy_overlay(gameJson, nationId));
  if (parsed.error || !parsed.relations) return null;
  return parsed;
}

export function getNewspaperArchive(gameJson: string): ArchivedNewspaper[] {
  const parsed = JSON.parse(wasm_get_newspaper_archive(gameJson));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export function getPoliticalSnapshot(gameJson: string, turn: number): PoliticalSnapshot | null {
  const parsed = JSON.parse(wasm_get_political_snapshot(gameJson, turn));
  if (parsed.error) return null;
  return parsed;
}

export function getBattleArchive(gameJson: string): ArchivedBattleTurn[] {
  const parsed = JSON.parse(wasm_get_battle_data(gameJson));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export function getAllGPLedgerData(gameJson: string): GPLedgerEntry[] {
  const parsed = JSON.parse(wasm_get_all_gp_ledger_data(gameJson));
  if (parsed.error || !Array.isArray(parsed)) return [];
  return parsed;
}

export function getLedgerData(gameJson: string, nationId: number): LedgerData | null {
  const parsed = JSON.parse(wasm_get_ledger_data(gameJson, nationId));
  if (parsed.error) return null;
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

export type EngineerBuildKind = 'railroad' | 'depot' | 'port';

export function engineerBuild(gameJson: string, civilianId: number, kind: EngineerBuildKind): CommandResult {
  return executeCommand(wasm_engineer_build(gameJson, civilianId, kind));
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

// ── Transport types & functions ─────────────────────────────────────

export interface TransportAllocation {
  resource: string;
  percentage: number;
}

export interface TransportDelivery {
  resource: string;
  available: number;
  delivered: number;
}

export interface TransportData {
  freight_cars: number;
  total_capacity: number;
  military_transport_capacity: number;
  allocations: TransportAllocation[];
  build_cost: { labor: number; lumber: number; steel: number };
  can_build: boolean;
  available_lumber: number;
  available_steel: number;
  available_labor: number;
  deliveries: TransportDelivery[];
}

export function getTransportData(gameJson: string, nationId: number): TransportData | null {
  const parsed = JSON.parse(wasm_get_transport_data(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function buildFreightCar(gameJson: string, nationId: number): CommandResult {
  return executeCommand(wasm_build_freight_car(gameJson, nationId));
}

export function setTransportAllocation(gameJson: string, nationId: number, resource: string, percentage: number): CommandResult {
  return executeCommand(wasm_set_transport_allocation(gameJson, nationId, resource, percentage));
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

export interface ProductionForecast {
  mill_output: number;
  factory_output: number;
  mill_labor: number;
  factory_labor: number;
}

export interface IndustryData {
  buildings: BuildingInfo[];
  warehouse: {
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
  };
  production_forecast: {
    timber_chain: ProductionForecast;
    metal_chain: ProductionForecast;
    textile_chain: ProductionForecast;
  };
  can_expand: Record<string, boolean>;
}

export function getIndustryData(gameJson: string, nationId: number): IndustryData | null {
  const parsed = JSON.parse(wasm_get_industry_data(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function expandBuilding(gameJson: string, nationId: number, buildingType: string): CommandResult {
  return executeCommand(wasm_expand_building(gameJson, nationId, buildingType));
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
  resource: string;
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
  sellable_materials: SellableItem[];
  sellable_goods: SellableItem[];
}

export function getTradeData(gameJson: string, nationId: number): TradeData | null {
  const parsed = JSON.parse(wasm_get_trade_data(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function setTradeSubsidy(gameJson: string, nationId: number, targetNationId: number, amount: number): CommandResult {
  return executeCommand(wasm_set_trade_subsidy(gameJson, nationId, targetNationId, BigInt(amount)));
}

export function setPlayerSellOrder(gameJson: string, nationId: number, commodityType: string, commodityName: string, quantity: number): CommandResult {
  return executeCommand(wasm_set_player_sell_order(gameJson, nationId, commodityType, commodityName, quantity));
}

export function setPlayerBuyOrder(gameJson: string, nationId: number, resource: string, quantity: number, maxPrice: number): CommandResult {
  return executeCommand(wasm_set_player_buy_order(gameJson, nationId, resource, quantity, BigInt(maxPrice)));
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
  has_pending_nap: boolean;
  has_pending_alliance: boolean;
  has_pending_peace: boolean;
  is_in_anarchy: boolean;
  actions: DiplomacyRelationActions;
}

export interface DiplomacyScreenData {
  player_standing: number;
  treasury: number;
  player_already_at_war: boolean;
  relations: DiplomacyScreenRelation[];
}

export function getDiplomacyScreenData(gameJson: string, nationId: number): DiplomacyScreenData | null {
  const parsed = JSON.parse(wasm_get_diplomacy_screen_data(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function diplomacyBuildConsulate(gameJson: string, nationId: number, targetId: number): CommandResult {
  return executeCommand(wasm_diplomacy_build_consulate(gameJson, nationId, targetId));
}

export function diplomacyBuildEmbassy(gameJson: string, nationId: number, targetId: number): CommandResult {
  return executeCommand(wasm_diplomacy_build_embassy(gameJson, nationId, targetId));
}

export function diplomacyProposeNap(gameJson: string, nationId: number, targetId: number): CommandResult {
  return executeCommand(wasm_diplomacy_propose_nap(gameJson, nationId, targetId));
}

export function diplomacyProposeAlliance(gameJson: string, nationId: number, targetId: number): CommandResult {
  return executeCommand(wasm_diplomacy_propose_alliance(gameJson, nationId, targetId));
}

export function diplomacyDeclareWar(gameJson: string, nationId: number, targetId: number): CommandResult {
  return executeCommand(wasm_diplomacy_declare_war(gameJson, nationId, targetId));
}

export function diplomacySendGrant(gameJson: string, nationId: number, targetId: number, amount: number): CommandResult {
  return executeCommand(wasm_diplomacy_send_grant(gameJson, nationId, targetId, BigInt(amount)));
}

export function diplomacyBreakTreaty(gameJson: string, nationId: number, targetId: number, treatyType: string): CommandResult {
  return executeCommand(wasm_diplomacy_break_treaty(gameJson, nationId, targetId, treatyType));
}

export function diplomacyProposePeace(gameJson: string, nationId: number, targetId: number): CommandResult {
  return executeCommand(wasm_diplomacy_propose_peace(gameJson, nationId, targetId));
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

export function getPendingProposals(gameJson: string, nationId: number): ProposalData | null {
  const parsed = JSON.parse(wasm_get_pending_proposals(gameJson, nationId));
  if (parsed.error) return null;
  return parsed;
}

export function acceptProposal(gameJson: string, nationId: number, proposalIndex: number): CommandResult {
  return executeCommand(wasm_accept_proposal(gameJson, nationId, proposalIndex));
}

export function rejectProposal(gameJson: string, nationId: number, proposalIndex: number): CommandResult {
  return executeCommand(wasm_reject_proposal(gameJson, nationId, proposalIndex));
}
