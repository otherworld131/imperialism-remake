import { describe, it, expect } from 'vitest';
import { fbm, noise2, chaikin, displaceAlongNormal, stitchPolylines, organicPolyline } from './mapGeometry';

describe('mapGeometry: value noise', () => {
  it('is deterministic', () => {
    expect(noise2(3.7, -1.2, 42)).toBe(noise2(3.7, -1.2, 42));
    expect(fbm(3.7, -1.2, 3, 42)).toBe(fbm(3.7, -1.2, 3, 42));
  });

  it('changes with the seed', () => {
    expect(noise2(3.7, -1.2, 1)).not.toBe(noise2(3.7, -1.2, 2));
  });

  it('stays roughly in [-1, 1]', () => {
    for (let i = 0; i < 50; i++) {
      const v = fbm(Math.random() * 100, Math.random() * 100, 3, 7);
      expect(v).toBeGreaterThanOrEqual(-1.2);
      expect(v).toBeLessThanOrEqual(1.2);
    }
  });
});

describe('mapGeometry: Chaikin', () => {
  it('preserves endpoints for open polylines', () => {
    const pts: [number, number][] = [[0, 0], [10, 0], [10, 10]];
    const out = chaikin(pts, 3, false);
    expect(out[0]).toEqual([0, 0]);
    expect(out[out.length - 1]).toEqual([10, 10]);
    expect(out.length).toBeGreaterThan(pts.length);
  });

  it('quadruples vertex count on closed loops', () => {
    const square: [number, number][] = [[0, 0], [10, 0], [10, 10], [0, 10]];
    const out = chaikin(square, 2, true);
    expect(out.length).toBe(square.length * 4);
  });
});

describe('mapGeometry: displaceAlongNormal', () => {
  it('inserts subdiv-1 interior points per segment on open polylines', () => {
    const line: [number, number][] = [[0, 0], [10, 0]];
    const out = displaceAlongNormal(line, {
      subdiv: 4, amplitude: 1, frequency: 0.1, octaves: 1, seed: 1, closed: false,
    });
    expect(out.length).toBe(2 + (4 - 1)); // endpoints + interior
  });

  it('keeps endpoints fixed', () => {
    const line: [number, number][] = [[0, 0], [10, 0]];
    const out = displaceAlongNormal(line, {
      subdiv: 4, amplitude: 5, frequency: 0.2, octaves: 2, seed: 1, closed: false,
    });
    expect(out[0]).toEqual([0, 0]);
    expect(out[out.length - 1]).toEqual([10, 0]);
  });

  it('actually moves interior points off the original line', () => {
    const line: [number, number][] = [[0, 0], [10, 0]];
    const out = displaceAlongNormal(line, {
      subdiv: 8, amplitude: 5, frequency: 0.3, octaves: 2, seed: 7, closed: false,
    });
    const maxOffset = Math.max(...out.slice(1, -1).map(([, y]) => Math.abs(y)));
    expect(maxOffset).toBeGreaterThan(0);
  });
});

describe('mapGeometry: stitchPolylines', () => {
  it('stitches a simple closed triangle', () => {
    // Edges of a triangle (vertices a,b,c)
    const edges = [
      { a: 'a', b: 'b' },
      { a: 'b', b: 'c' },
      { a: 'c', b: 'a' },
    ];
    const { closed, open } = stitchPolylines(edges);
    expect(open).toHaveLength(0);
    expect(closed).toHaveLength(1);
    expect(closed[0]).toHaveLength(3);
    // Should contain all three vertices exactly once
    expect(new Set(closed[0])).toEqual(new Set(['a', 'b', 'c']));
  });

  it('stitches an open polyline with two endpoints', () => {
    const edges = [
      { a: 'a', b: 'b' },
      { a: 'b', b: 'c' },
      { a: 'c', b: 'd' },
    ];
    const { closed, open } = stitchPolylines(edges);
    expect(closed).toHaveLength(0);
    expect(open).toHaveLength(1);
    const p = open[0];
    expect(p[0] === 'a' || p[0] === 'd').toBe(true);
    expect(p[p.length - 1] === 'a' || p[p.length - 1] === 'd').toBe(true);
    expect(p[0]).not.toBe(p[p.length - 1]);
    expect(p).toHaveLength(4);
  });

  it('separates disjoint closed loops', () => {
    const edges = [
      // Loop 1: a-b-c
      { a: 'a', b: 'b' }, { a: 'b', b: 'c' }, { a: 'c', b: 'a' },
      // Loop 2: x-y-z-w
      { a: 'x', b: 'y' }, { a: 'y', b: 'z' }, { a: 'z', b: 'w' }, { a: 'w', b: 'x' },
    ];
    const { closed } = stitchPolylines(edges);
    expect(closed).toHaveLength(2);
    const sizes = closed.map(l => l.length).sort();
    expect(sizes).toEqual([3, 4]);
  });
});

describe('mapGeometry: organicPolyline smoke', () => {
  it('produces more vertices than input and preserves open-endpoint positions', () => {
    const line: [number, number][] = [[0, 0], [20, 0], [20, 20]];
    const out = organicPolyline(line, { amplitude: 2, frequency: 0.1, subdiv: 6, smoothing: 2, closed: false });
    expect(out.length).toBeGreaterThan(line.length * 4);
    // Chaikin-with-open-endpoint-preservation keeps the endpoint unchanged.
    expect(out[0]).toEqual([0, 0]);
    expect(out[out.length - 1]).toEqual([20, 20]);
  });
});
