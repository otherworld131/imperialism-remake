//! Zoom-based level-of-detail gates, mirroring the React frontend's
//! `scale > X` checks. React's `scale` is screen px per world unit at
//! `HEX_SIZE = 18`; Bevy's orthographic `scale` is world units per screen px
//! at `HEX_SIZE = 24`, so the equivalent React scale is
//! `24 / (18 * ortho_scale)`.

use bevy::prelude::*;

use crate::map::camera::GameCamera;
use crate::map::geometry::HEX_SIZE;
use crate::map::layers::MapBounds;

/// Hex size the React gate constants were authored against.
const REACT_HEX_SIZE: f32 = 18.0;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZoomLod {
    /// Resource icons (React `scale > 0.6`).
    pub resources: bool,
    /// Infrastructure icons (React `scale > 0.8`).
    pub infra: bool,
    /// Civilian markers (React `scale > 0.7`).
    pub civilians: bool,
    /// Troop indicators (React `scale > 0.6`).
    pub troops: bool,
    /// Hex grid + sea zone labels (React `scale > 0.4`).
    pub grid: bool,
    /// Zoomed in past the nation-label threshold (`scale > fit * 1.5`):
    /// province labels + province borders replace nation labels.
    pub past_labels: bool,
}

/// React-equivalent scale for a Bevy orthographic scale.
pub fn react_scale(ortho_scale: f32) -> f32 {
    HEX_SIZE / (REACT_HEX_SIZE * ortho_scale)
}

/// Pure gate computation. `fit_react_scale` is the React-equivalent scale at
/// which the map's full height exactly fills the viewport.
pub fn compute_lod(rs: f32, fit_react_scale: f32) -> ZoomLod {
    ZoomLod {
        resources: rs > 0.6,
        infra: rs > 0.8,
        civilians: rs > 0.7,
        troops: rs > 0.6,
        grid: rs > 0.4,
        past_labels: fit_react_scale > 0.0 && rs > fit_react_scale * 1.5,
    }
}

/// Layers gated by a [`ZoomLod`] flag carry this component; visibility is
/// flipped per frame (cheap — writes only on change).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodGate {
    Resources,
    Infra,
    Civilians,
    Troops,
    Grid,
    /// Visible only when zoomed in past the label threshold.
    PastLabels,
    /// Visible only when NOT zoomed in past the label threshold.
    NotPastLabels,
}

pub fn update_zoom_lod(
    camera: Query<(&Projection, &Camera), With<GameCamera>>,
    bounds: Option<Res<MapBounds>>,
    windows: Query<&Window>,
    mut lod: ResMut<ZoomLod>,
) {
    let Ok((projection, _)) = camera.single() else {
        return;
    };
    let Projection::Orthographic(ortho) = projection else {
        return;
    };
    let rs = react_scale(ortho.scale);
    // Fit scale: ortho scale at which the map height fills the window.
    let fit_rs = match (bounds.as_deref(), windows.single().ok()) {
        (Some(b), Some(window)) if window.height() > 0.0 => {
            // Map pixel height in React convention includes half-hex caps.
            let map_height_world = (b.max.y - b.min.y) + 2.0 * HEX_SIZE;
            if map_height_world > 0.0 {
                react_scale(map_height_world / window.height())
            } else {
                0.0
            }
        }
        _ => 0.0,
    };
    let next = compute_lod(rs, fit_rs);
    if *lod != next {
        *lod = next;
    }
}

/// Runs every frame (not just on LOD change) so freshly rebuilt layers get
/// their gate applied immediately; the inner write is change-detected.
pub fn apply_lod_gates(lod: Res<ZoomLod>, mut gated: Query<(&LodGate, &mut Visibility)>) {
    for (gate, mut visibility) in &mut gated {
        let show = match gate {
            LodGate::Resources => lod.resources,
            LodGate::Infra => lod.infra,
            LodGate::Civilians => lod.civilians,
            LodGate::Troops => lod.troops,
            LodGate::Grid => lod.grid,
            LodGate::PastLabels => lod.past_labels,
            LodGate::NotPastLabels => !lod.past_labels,
        };
        let target = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != target {
            *visibility = target;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn react_scale_round_trips_hex_ratio() {
        // ortho scale 1.0 → hexes render 24 screen px ↔ React scale 24/18.
        assert!((react_scale(1.0) - 24.0 / 18.0).abs() < 1e-6);
        // The gate "React scale > 0.6" flips at ortho 24/(18*0.6) ≈ 2.222.
        assert!(compute_lod(react_scale(2.2), 0.0).resources);
        assert!(!compute_lod(react_scale(2.3), 0.0).resources);
    }

    #[test]
    fn gates_match_react_thresholds() {
        let lod = compute_lod(0.75, 0.6);
        assert!(lod.resources && lod.troops && lod.civilians && lod.grid);
        assert!(!lod.infra);
        // 0.75 > 0.6 * 1.5 is false (0.9 boundary not exceeded).
        assert!(!lod.past_labels);
        let lod = compute_lod(0.95, 0.6);
        assert!(lod.infra);
        assert!(lod.past_labels);
    }

    #[test]
    fn past_labels_requires_known_fit_scale() {
        assert!(!compute_lod(10.0, 0.0).past_labels);
    }
}
