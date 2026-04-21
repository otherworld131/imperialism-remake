import { describe, expect, it } from 'vitest';
import type { TileData } from '../wasm';
import { computeNationLabels } from './nationLabels';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);
function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
}

function tile(q: number, r: number, owner: string): TileData {
  return {
    q, r,
    terrain: 'Plain', resource: null, resource_hidden: false,
    is_capital: false, is_country_capital: false,
    improvement_level: 0, max_improvement_level: 0,
    owner, owner_color: 'Red', province: 'P',
    province_id: null,
    has_railroad: false, has_depot: false, has_port: false,
    has_fort: false, fort_level: 0,
    map_width: 10,
    nation_id: 0,
    army_firepower: 0, army_unit_count: 0, army_composition: null,
    naval_firepower: 0, naval_ship_count: 0,
    civilian_on_tile: null,
    is_minor: false, is_incorporated_minor: false, is_anarchic: false,
    visual_group: null, visible: true,
  };
}

describe('computeNationLabels', () => {
  it('places label on one of the component hexes for a compact shape', () => {
    // 3-hex straight line — small enough to hit minSize=3 exactly
    const tiles = [tile(0, 0, 'A'), tile(1, 0, 'A'), tile(2, 0, 'A')];
    const labels = computeNationLabels(tiles);
    expect(labels).toHaveLength(1);
    const label = labels[0];
    expect(label.name).toBe('A');
    const hexPixels = tiles.map(t => hexToPixel(t.q, t.r));
    const match = hexPixels.find(([x, y]) =>
      Math.abs(x - label.cx) < 1e-6 && Math.abs(y - label.cy) < 1e-6
    );
    expect(match, 'label position must coincide with one of the component hex centers').toBeDefined();
  });

  it('on a concave L-shape whose centroid is outside the territory, snaps to an owned hex', () => {
    // L-shape of owner 'L': hexes at (0,0), (1,0), (2,0), (2,1), (2,2). Centroid biases toward (2,0)/(2,1).
    // Surround with owner 'O' hexes that are NOT part of the L, including one near the centroid.
    const lHexes: [number, number][] = [[0, 0], [1, 0], [2, 0], [2, 1], [2, 2]];
    const otherHexes: [number, number][] = [
      // A non-L hex that sits near the centroid of the L
      [1, 1],
      // Enough other-owner hexes to form a distinct nation if needed
      [3, 0], [3, 1], [3, 2],
    ];
    const tiles: TileData[] = [
      ...lHexes.map(([q, r]) => tile(q, r, 'L')),
      ...otherHexes.map(([q, r]) => tile(q, r, 'O')),
    ];
    const labels = computeNationLabels(tiles, 3);
    const lLabel = labels.find(l => l.name === 'L');
    expect(lLabel, 'expected a label for nation L').toBeDefined();
    const lPixels = lHexes.map(([q, r]) => hexToPixel(q, r));
    const match = lPixels.find(([x, y]) =>
      Math.abs(x - lLabel!.cx) < 1e-6 && Math.abs(y - lLabel!.cy) < 1e-6
    );
    expect(match, "L's label must snap to an L-owned hex, not to (1,1) which is owned by O").toBeDefined();
    // Sanity: the raw centroid (1.4, ~) is NOT the label position — snapping must have moved it.
    const cx = lPixels.reduce((a, [x]) => a + x, 0) / lPixels.length;
    const cy = lPixels.reduce((a, [, y]) => a + y, 0) / lPixels.length;
    const snappedToCentroid = Math.abs(lLabel!.cx - cx) < 1e-6 && Math.abs(lLabel!.cy - cy) < 1e-6;
    expect(snappedToCentroid).toBe(false);
  });

  it('skips components smaller than minSize', () => {
    const tiles = [tile(0, 0, 'Tiny'), tile(1, 0, 'Tiny')];
    const labels = computeNationLabels(tiles, 3);
    expect(labels).toHaveLength(0);
  });
});
