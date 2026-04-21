import type { TileData } from '../wasm';

const HEX_SIZE = 18;
const SQRT3 = Math.sqrt(3);

function hexToPixel(q: number, r: number): [number, number] {
  return [HEX_SIZE * (SQRT3 * q + SQRT3 / 2 * r), HEX_SIZE * (3 / 2 * r)];
}

function hexNeighbors(q: number, r: number): [number, number][] {
  return [
    [q + 1, r], [q, r + 1], [q - 1, r + 1],
    [q - 1, r], [q, r - 1], [q + 1, r - 1],
  ];
}

export interface NationLabel {
  name: string;
  cx: number;
  cy: number;
  size: number;
  is_anarchic: boolean;
}

/**
 * Group land tiles into BFS-connected components per visual_group (or owner)
 * and return a label per component large enough to warrant one.
 */
export function computeNationLabels(tiles: TileData[], minSize = 3): NationLabel[] {
  const labels: NationLabel[] = [];
  const nationTiles = new Map<string, { tiles: Set<string>; is_anarchic: boolean }>();
  for (const tile of tiles) {
    if (tile.terrain === 'Sea' || !tile.owner) continue;
    const key = `${tile.q},${tile.r}`;
    const groupName = tile.visual_group || tile.owner;
    let entry = nationTiles.get(groupName);
    if (!entry) {
      entry = { tiles: new Set(), is_anarchic: tile.is_anarchic };
      nationTiles.set(groupName, entry);
    }
    entry.tiles.add(key);
  }

  for (const [name, entry] of nationTiles) {
    const visited = new Set<string>();
    for (const startKey of entry.tiles) {
      if (visited.has(startKey)) continue;
      const component: string[] = [];
      const queue: string[] = [startKey];
      let head = 0;
      visited.add(startKey);
      while (head < queue.length) {
        const cur = queue[head++];
        component.push(cur);
        const [cq, cr] = cur.split(',').map(Number);
        const nbrs = hexNeighbors(cq, cr);
        for (const [nq, nr] of nbrs) {
          const nk = `${nq},${nr}`;
          if (!visited.has(nk) && entry.tiles.has(nk)) {
            visited.add(nk);
            queue.push(nk);
          }
        }
      }
      if (component.length < minSize) continue;
      let sx = 0, sy = 0;
      const pixels: [number, number][] = [];
      for (const k of component) {
        const [cq, cr] = k.split(',').map(Number);
        const [px, py] = hexToPixel(cq, cr);
        pixels.push([px, py]);
        sx += px; sy += py;
      }
      const centroidX = sx / component.length;
      const centroidY = sy / component.length;
      let bestPx = pixels[0][0], bestPy = pixels[0][1];
      let bestDist = Infinity;
      for (const [px, py] of pixels) {
        const dx = px - centroidX, dy = py - centroidY;
        const d = dx * dx + dy * dy;
        if (d < bestDist) { bestDist = d; bestPx = px; bestPy = py; }
      }
      labels.push({
        name,
        cx: bestPx,
        cy: bestPy,
        size: component.length,
        is_anarchic: entry.is_anarchic,
      });
    }
  }
  return labels;
}
