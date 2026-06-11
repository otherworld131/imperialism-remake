//! Navy-marker placement helpers — port of `web/src/components/HexMap.helpers.ts`.
//! Sizes are expressed in React world units (hex 18) and scaled by the
//! caller; the offsets here already scale with the hex ratio.

use crate::game::vm::NavyMarker;
use crate::map::geometry::HEX_SIZE;
use std::collections::HashMap;

/// React hex size the marker constants were authored against.
const REACT_HEX_SIZE: f32 = 18.0;
/// World-unit scale factor from React units to ours.
pub const REACT_SCALE: f32 = HEX_SIZE / REACT_HEX_SIZE;

/// Visual radius of a navy marker badge (11 React units).
pub const NAVY_MARKER_RADIUS: f32 = 11.0 * REACT_SCALE;
/// Base radius of the overlap-avoidance spiral (9 React units).
pub const NAVY_MARKER_OFFSET_BASE: f32 = 9.0 * REACT_SCALE;
/// Golden angle in radians — successive indices never share a spoke.
pub const GOLDEN_ANGLE_RAD: f32 = std::f32::consts::PI * (3.0 - 2.236_068);

/// Stable identity for a navy marker — used for selection comparison.
pub fn marker_key(m: &NavyMarker) -> String {
    if m.kind == "beachhead"
        && let Some(target) = m.target_hex
    {
        return format!("b:{}:{},{}", m.nation_id, target.q, target.r);
    }
    format!("f:{}", m.nation_id)
}

/// Deterministic offset for overlapping markers at the same anchor hex:
/// marker #0 at the center, later ones on a golden-angle spiral with radius
/// `base * sqrt(index)`. In React pixel convention (y down).
pub fn marker_offset(index: usize) -> (f32, f32) {
    if index == 0 {
        return (0.0, 0.0);
    }
    let angle = index as f32 * GOLDEN_ANGLE_RAD;
    let radius = NAVY_MARKER_OFFSET_BASE * (index as f32).sqrt();
    (radius * angle.cos(), radius * angle.sin())
}

/// Order-stable index of each marker within its anchor hex. Draw and
/// hit-test consume the same map so they stay in sync.
pub fn anchor_index_map(markers: &[NavyMarker]) -> HashMap<String, usize> {
    let mut seen: HashMap<(i32, i32), usize> = HashMap::new();
    let mut out = HashMap::new();
    for m in markers {
        let n = seen.entry((m.q, m.r)).or_insert(0);
        out.insert(marker_key(m), *n);
        *n += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn marker(nation_id: i64, q: i32, r: i32, kind: &str) -> NavyMarker {
        NavyMarker {
            q,
            r,
            nation_id,
            owner_name: "X".into(),
            owner_color: "Red".into(),
            kind: kind.into(),
            ship_count: 1,
            total_fp: 1,
            total_hull: 1,
            by_type: BTreeMap::new(),
            by_operation: BTreeMap::new(),
            visible: true,
            sea_zone_id: None,
            sea_zone_name: None,
            pending_move_to_zone_id: None,
            target_province: None,
            target_hex: None,
        }
    }

    #[test]
    fn first_marker_sits_at_center() {
        assert_eq!(marker_offset(0), (0.0, 0.0));
    }

    #[test]
    fn spiral_radius_grows_with_sqrt_index() {
        let (x1, y1) = marker_offset(1);
        let (x4, y4) = marker_offset(4);
        let r1 = x1.hypot(y1);
        let r4 = x4.hypot(y4);
        assert!((r1 - NAVY_MARKER_OFFSET_BASE).abs() < 1e-4);
        assert!((r4 - NAVY_MARKER_OFFSET_BASE * 2.0).abs() < 1e-4);
        // Golden-angle spokes never coincide for small indices.
        let a1 = y1.atan2(x1);
        let a4 = y4.atan2(x4);
        assert!((a1 - a4).abs() > 1e-3);
    }

    #[test]
    fn anchor_indices_count_per_hex() {
        let markers = vec![
            marker(0, 5, 5, "fleet"),
            marker(1, 5, 5, "fleet"),
            marker(2, 9, 9, "fleet"),
        ];
        let map = anchor_index_map(&markers);
        assert_eq!(map[&marker_key(&markers[0])], 0);
        assert_eq!(map[&marker_key(&markers[1])], 1);
        assert_eq!(map[&marker_key(&markers[2])], 0);
    }
}
