import type { CapitalOverride, TileData } from '../wasm';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);
const HEX_HALF_WIDTH = HEX_SIZE * SQRT3 * 0.5;
const VIEW_PADDING = 48;

const SURFACE_RESOURCE_YIELD = 2;
const HEAVY_DEPOSIT_YIELD = 2;
const PRECIOUS_DEPOSIT_YIELD = 1;
const RESOURCE_ORDER = [
  'Grain', 'Fruit', 'Livestock', 'Fish',
  'Timber', 'Cotton', 'Wool', 'Horses',
  'Coal', 'Iron', 'Gold', 'Gems', 'Oil',
];

export interface CapitalResourceEntry {
  resource: string;
  amount: number;
}

export interface CapitalFoodSupply {
  grain: number;
  fruit: number;
  meat: number;
}

export interface CapitalSitePreview {
  capital: CapitalOverride;
  support: number | null;
  foodSupply: CapitalFoodSupply;
  resources: CapitalResourceEntry[];
  collectedTiles: number;
}

export interface ViewportSize {
  width: number;
  height: number;
}

export interface PreviewTransform {
  scale: number;
  offset: { x: number; y: number };
}

export function tileKey(q: number, r: number): string {
  return `${q},${r}`;
}

export function isValidCapitalTile(tile: TileData | null, nationId: number | null): tile is TileData {
  return tile != null
    && nationId != null
    && tile.nation_id === nationId
    && tile.terrain !== 'Sea'
    && tile.terrain !== 'Mountain';
}

function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
}

function resourceYieldAtCapitalStart(resource: string): number {
  if (resource === 'Coal' || resource === 'Iron' || resource === 'Oil') {
    return HEAVY_DEPOSIT_YIELD;
  }
  if (resource === 'Gold' || resource === 'Gems') {
    return PRECIOUS_DEPOSIT_YIELD;
  }
  return SURFACE_RESOURCE_YIELD;
}

function addResource(resources: Map<string, number>, resource: string, amount: number): void {
  if (amount <= 0) return;
  resources.set(resource, (resources.get(resource) ?? 0) + amount);
}

export function evaluateCapitalSite(
  center: TileData | null,
  tileByCoord: Map<string, TileData>,
  nationId: number | null,
): CapitalSitePreview | null {
  if (!isValidCapitalTile(center, nationId)) return null;

  const coords: Array<[number, number]> = [
    [center.q, center.r],
    [center.q + 1, center.r],
    [center.q + 1, center.r - 1],
    [center.q, center.r - 1],
    [center.q - 1, center.r],
    [center.q - 1, center.r + 1],
    [center.q, center.r + 1],
  ];
  const coastalNeighbors = coords.slice(1);
  const resources = new Map<string, number>();
  let collectedTiles = 0;
  let grain = 0;
  let fruit = 0;
  let meat = 0;

  for (const [q, r] of coords) {
    const tile = tileByCoord.get(tileKey(q, r));
    if (!tile || tile.nation_id !== nationId || tile.terrain === 'Sea') continue;

    collectedTiles += 1;
    if (tile.resource) {
      const qty = resourceYieldAtCapitalStart(tile.resource);
      addResource(resources, tile.resource, qty);
      if (tile.resource === 'Grain') grain += qty;
      else if (tile.resource === 'Fruit') fruit += qty;
      else if (tile.resource === 'Livestock') meat += qty;
    } else if (tile.terrain === 'Grassland') {
      addResource(resources, 'Grain', 1);
      grain += 1;
    }

    if (tile.has_river) {
      addResource(resources, 'Fish', 1);
      meat += 1;
    }
  }

  let coastalFish = 0;
  for (const [q, r] of coastalNeighbors) {
    const tile = tileByCoord.get(tileKey(q, r));
    if (tile?.terrain === 'Sea') coastalFish += 1;
  }
  coastalFish = Math.min(coastalFish, 3);
  if (coastalFish > 0) {
    addResource(resources, 'Fish', coastalFish);
    meat += coastalFish;
  }

  const orderedResources = RESOURCE_ORDER
    .map(resource => ({ resource, amount: resources.get(resource) ?? 0 }))
    .filter(entry => entry.amount > 0);

  return {
    capital: { q: center.q, r: center.r },
    support: null,
    foodSupply: { grain, fruit, meat },
    resources: orderedResources,
    collectedTiles,
  };
}

export function computeNationPlacementView(
  tiles: TileData[],
  nationId: number | null,
  viewport: ViewportSize,
): PreviewTransform | null {
  if (nationId == null || viewport.width <= 0 || viewport.height <= 0) return null;
  const nationTiles = tiles.filter(tile => tile.nation_id === nationId && tile.terrain !== 'Sea');
  if (nationTiles.length === 0) return null;

  let minX = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const tile of nationTiles) {
    const [px, py] = hexToPixel(tile.q, tile.r);
    minX = Math.min(minX, px - HEX_HALF_WIDTH);
    maxX = Math.max(maxX, px + HEX_HALF_WIDTH);
    minY = Math.min(minY, py - HEX_SIZE);
    maxY = Math.max(maxY, py + HEX_SIZE);
  }

  const worldWidth = Math.max(HEX_HALF_WIDTH * 2, maxX - minX);
  const worldHeight = Math.max(HEX_SIZE * 2, maxY - minY);
  const usableWidth = Math.max(120, viewport.width - VIEW_PADDING * 2);
  const usableHeight = Math.max(120, viewport.height - VIEW_PADDING * 2);
  const scale = Math.min(4, Math.min(usableWidth / worldWidth, usableHeight / worldHeight));
  const centerX = (minX + maxX) * 0.5;
  const centerY = (minY + maxY) * 0.5;

  return {
    scale,
    offset: {
      x: viewport.width * 0.5 - centerX * scale,
      y: viewport.height * 0.5 - centerY * scale,
    },
  };
}
