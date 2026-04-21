import { describe, expect, it } from 'vitest';
import type { NavyMarker } from '../wasm';
import { navyMarkerKey, navyMarkerOffset } from './HexMap.helpers';

function fleetMarker(nationId: number): NavyMarker {
  return {
    q: 0, r: 0,
    nation_id: nationId,
    owner_name: `Nation${nationId}`,
    owner_color: 'Red',
    kind: 'fleet',
    ship_count: 1,
    total_fp: 10,
    total_hull: 25,
    by_type: { Frigate: 1 },
    by_operation: { Patrol: 1 },
    visible: true,
  };
}

function beachheadMarker(nationId: number, targetHex: { q: number; r: number }): NavyMarker {
  return {
    ...fleetMarker(nationId),
    kind: 'beachhead',
    target_province: 'Kirkenes',
    target_hex: targetHex,
  };
}

describe('navyMarkerKey', () => {
  it('identifies fleet markers by nation', () => {
    expect(navyMarkerKey(fleetMarker(2))).toBe('f:2');
    expect(navyMarkerKey(fleetMarker(3))).toBe('f:3');
  });

  it('identifies beachhead markers by nation + target hex', () => {
    const a = beachheadMarker(2, { q: 5, r: -1 });
    const b = beachheadMarker(2, { q: 7, r: 0 });
    expect(navyMarkerKey(a)).toBe('b:2:5,-1');
    expect(navyMarkerKey(b)).toBe('b:2:7,0');
    expect(navyMarkerKey(a)).not.toBe(navyMarkerKey(b));
  });

  it('fleet and beachhead keys are always disjoint for the same nation', () => {
    const fleet = fleetMarker(4);
    const bh = beachheadMarker(4, { q: 1, r: 2 });
    expect(navyMarkerKey(fleet)).not.toBe(navyMarkerKey(bh));
  });
});

describe('navyMarkerOffset (golden-angle spiral)', () => {
  it('puts index 0 at the origin', () => {
    expect(navyMarkerOffset(0)).toEqual([0, 0]);
  });

  it('produces well-separated offsets for the first 100 indices', () => {
    // Golden-angle spiral: any two offsets are non-zero distance apart and
    // the minimum pairwise separation is bounded below by a small epsilon
    // (degenerates only at index 0, which is the origin and is excluded from
    // the pairwise sweep).
    const pts: Array<[number, number]> = [];
    for (let i = 1; i < 100; i++) pts.push(navyMarkerOffset(i));
    const EPS = 1e-3;
    for (let i = 0; i < pts.length; i++) {
      for (let j = i + 1; j < pts.length; j++) {
        const dx = pts[i][0] - pts[j][0];
        const dy = pts[i][1] - pts[j][1];
        const sep = Math.hypot(dx, dy);
        expect(sep).toBeGreaterThan(EPS);
      }
    }
  });

  it('radius grows monotonically', () => {
    let prev = 0;
    for (let i = 1; i < 12; i++) {
      const [x, y] = navyMarkerOffset(i);
      const r = Math.hypot(x, y);
      expect(r).toBeGreaterThanOrEqual(prev - 1e-9);
      prev = r;
    }
  });
});
