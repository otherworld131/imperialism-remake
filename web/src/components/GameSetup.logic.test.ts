import { describe, expect, it } from 'vitest';
import type { TileData } from '../wasm';
import { computeNationPlacementView, evaluateCapitalSite } from './GameSetup.logic';

function makeTile(overrides: Partial<TileData> = {}): TileData {
  return {
    q: 0,
    r: 0,
    terrain: 'Grassland',
    resource: null,
    resource_hidden: false,
    has_river: false,
    is_capital: false,
    is_country_capital: false,
    improvement_level: 0,
    max_improvement_level: 0,
    owner: 'Player',
    owner_color: 'Red',
    province: 'Capital',
    province_id: 1,
    has_railroad: false,
    has_depot: false,
    has_port: false,
    port_blockaded: false,
    has_fort: false,
    fort_level: 0,
    map_width: 80,
    map_height: 50,
    nation_id: 1,
    army_firepower: 0,
    army_unit_count: 0,
    army_composition: null,
    naval_firepower: 0,
    naval_ship_count: 0,
    civilian_on_tile: null,
    is_minor: false,
    is_incorporated_minor: false,
    incorporated_nation_id: null,
    is_anarchic: false,
    visual_group: null,
    visible: true,
    is_prospected: true,
    ...overrides,
  };
}

describe('evaluateCapitalSite', () => {
  it('computes opening yields, support, and collection from owned tiles only', () => {
    const center = makeTile({ q: 0, r: 0 });
    const tiles = new Map<string, TileData>([
      ['0,0', center],
      ['1,0', makeTile({ q: 1, r: 0, resource: 'Grain', max_improvement_level: 3 })],
      ['1,-1', makeTile({ q: 1, r: -1, resource: 'Fruit', has_river: true, max_improvement_level: 3 })],
      ['0,-1', makeTile({ q: 0, r: -1, resource: 'Livestock', max_improvement_level: 3 })],
      ['-1,0', makeTile({ q: -1, r: 0, terrain: 'Hills', resource: 'Coal', resource_hidden: true, max_improvement_level: 3, is_prospected: false })],
      ['-1,1', makeTile({ q: -1, r: 1, terrain: 'Sea', nation_id: 0, province_id: null, owner: '' })],
      ['0,1', makeTile({ q: 0, r: 1, terrain: 'Sea', nation_id: 0, province_id: null, owner: '' })],
    ]);

    const preview = evaluateCapitalSite(center, tiles, 1);
    expect(preview).not.toBeNull();
    expect(preview?.collectedTiles).toBe(5);
    expect(preview?.support).toBe(6);
    expect(preview?.resources).toEqual([
      { resource: 'Grain', amount: 3 },
      { resource: 'Fruit', amount: 2 },
      { resource: 'Livestock', amount: 2 },
      { resource: 'Fish', amount: 3 },
      { resource: 'Coal', amount: 2 },
    ]);
  });

  it('rejects tiles outside the selected nation or invalid terrain', () => {
    const foreign = makeTile({ nation_id: 2 });
    const mountain = makeTile({ terrain: 'Mountain' });
    const map = new Map<string, TileData>([['0,0', foreign]]);

    expect(evaluateCapitalSite(foreign, map, 1)).toBeNull();
    expect(evaluateCapitalSite(mountain, new Map([['0,0', mountain]]), 1)).toBeNull();
  });
});

describe('computeNationPlacementView', () => {
  it('centers the selected nation in the requested viewport', () => {
    const tiles = [
      makeTile({ q: 0, r: 0, nation_id: 1 }),
      makeTile({ q: 3, r: 1, nation_id: 1 }),
      makeTile({ q: 10, r: 10, nation_id: 2 }),
    ];

    const view = computeNationPlacementView(tiles, 1, { width: 800, height: 600 });
    expect(view).not.toBeNull();
    expect(view!.scale).toBeGreaterThan(1);

    const centerX = (0 + (18 * (Math.sqrt(3) * 3 + Math.sqrt(3) / 2))) / 2;
    const centerY = (0 + (18 * 1.5)) / 2;
    const screenX = centerX * view!.scale + view!.offset.x;
    const screenY = centerY * view!.scale + view!.offset.y;
    expect(Math.abs(screenX - 400)).toBeLessThan(1);
    expect(Math.abs(screenY - 300)).toBeLessThan(1);
  });
});
