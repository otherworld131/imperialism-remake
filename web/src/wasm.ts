import init, {
  wasm_new_game,
  wasm_new_scenario_game,
  wasm_process_turn,
  wasm_get_map_data,
  wasm_get_available_techs,
  wasm_research_tech,
  wasm_get_scenarios,
} from '../../crates/wasm-bridge/pkg/wasm_bridge.js';

let initialized = false;

export async function initWasm() {
  if (!initialized) {
    await init();
    initialized = true;
  }
}

export interface TileData {
  q: number; r: number;
  terrain: string; resource: string | null; resource_hidden: boolean;
  is_capital: boolean;
  is_country_capital: boolean;
  improvement_level: number;
  owner: string; owner_color: string; province: string;
  has_railroad: boolean; has_depot: boolean; has_port: boolean;
  has_fort: boolean; fort_level: number;
  map_width: number;
}

export interface Headline {
  text: string;
  category: 'war' | 'battle' | 'diplomacy' | 'growth' | 'trade' | 'crisis' | 'politics' | 'military' | 'default';
}

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
