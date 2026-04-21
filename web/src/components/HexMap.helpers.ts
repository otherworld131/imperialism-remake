import type { NavyMarker } from '../wasm';

/** Visual radius (in canvas units) of a single navy marker badge. */
export const NAVY_MARKER_RADIUS = 11;

/** Base radius of the overlap-avoidance spiral. Successive markers at the
 *  same anchor hex are laid out on a golden-angle spiral with radius
 *  `base * sqrt(index)`. */
export const NAVY_MARKER_OFFSET_BASE = 9;

/** Golden angle in radians — 137.50776405…°. Successive indices never share
 *  a spoke. */
export const GOLDEN_ANGLE_RAD = Math.PI * (3 - Math.sqrt(5));

/** Stable identity for a navy marker — used for selection comparison. */
export function navyMarkerKey(m: NavyMarker): string {
  if (m.kind === 'beachhead' && m.target_hex) {
    return `b:${m.nation_id}:${m.target_hex.q},${m.target_hex.r}`;
  }
  return `f:${m.nation_id}`;
}

/** Deterministic offset for overlapping markers at the same anchor hex.
 *  Marker #0 sits at the hex center; subsequent markers are placed on a
 *  golden-angle spiral so no two indices ever land on the same spoke and
 *  the radius grows slowly enough to keep the fan visually compact.
 *  Draw and hit-test call the same function so they remain in sync. */
export function navyMarkerOffset(index: number): [number, number] {
  if (index <= 0) return [0, 0];
  const angle = index * GOLDEN_ANGLE_RAD;
  const radius = NAVY_MARKER_OFFSET_BASE * Math.sqrt(index);
  return [radius * Math.cos(angle), radius * Math.sin(angle)];
}
