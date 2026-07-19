//! 2D map camera: WASD/arrow pan, right/middle-drag pan, wheel and +/- key
//! zoom, Home reset, plus the horizontal-wrap teleport that keeps the camera
//! within half a world width of the center copy.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;

use crate::game::resources::CameraCentered;
use crate::map::layers::MapBounds;
use crate::map::picking::{PickingBlocker, cursor_over_ui};

#[derive(Component)]
pub struct GameCamera;

const CAMERA_SPEED: f32 = 760.0;
const MIN_ZOOM: f32 = 0.45;
const MAX_ZOOM: f32 = 2.8;
const DEFAULT_ZOOM: f32 = 1.25;
const VERTICAL_PADDING: f32 = 420.0;

/// Per-notch wheel-zoom sensitivity.
const WHEEL_ZOOM_STEP: f32 = 0.08;

/// New orthographic scale after a wheel notch. Returns `scale` unchanged when
/// the cursor is over a UI panel (`over_ui`) so scrolling the burger menu or
/// the setup sidebar never zooms the map behind them (Trello #543).
fn zoom_after_wheel(scale: f32, scroll_y: f32, over_ui: bool) -> f32 {
    if over_ui || scroll_y == 0.0 {
        return scale;
    }
    let factor = 1.0 - scroll_y * WHEEL_ZOOM_STEP;
    (scale * factor).clamp(MIN_ZOOM, MAX_ZOOM)
}

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Projection::from(OrthographicProjection {
            scale: DEFAULT_ZOOM,
            ..OrthographicProjection::default_2d()
        }),
        GameCamera,
    ));
}

/// Jump to the map center once the layer build publishes bounds. Session
/// swaps (restart / load / new preview world) reset [`CameraCentered`] and
/// remove the stale [`MapBounds`], so this re-centers once the new map's
/// bounds are published.
pub fn center_camera_when_map_ready(
    bounds: Option<Res<MapBounds>>,
    mut centered: ResMut<CameraCentered>,
    mut camera: Query<&mut Transform, With<GameCamera>>,
) {
    if centered.0 {
        return;
    }
    let Some(bounds) = bounds else {
        return;
    };
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    transform.translation.x = bounds.center.x;
    transform.translation.y = bounds.center.y;
    centered.0 = true;
}

pub fn camera_movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    bounds: Option<Res<MapBounds>>,
    blockers: Query<&Interaction, With<PickingBlocker>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ref mut orthographic) = *projection else {
        return;
    };
    // A focused text input owns the keyboard (web parity: HexMap ignores
    // keys typed into inputs); mouse pan/zoom stays live.
    let keyboard_free = focus.0.is_none();

    let mut direction = Vec2::ZERO;
    if keyboard_free {
        if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
            direction.y += 1.0;
        }
        if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
            direction.y -= 1.0;
        }
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            direction.x -= 1.0;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            direction.x += 1.0;
        }
    }
    if direction != Vec2::ZERO {
        let delta = direction.normalize() * CAMERA_SPEED * orthographic.scale * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
    }

    if mouse_buttons.pressed(MouseButton::Right) || mouse_buttons.pressed(MouseButton::Middle) {
        let delta = mouse_motion.delta * orthographic.scale;
        transform.translation.x -= delta.x;
        transform.translation.y += delta.y;
    }

    // Wheel zoom, but not when the pointer is over a UI panel that owns the
    // scroll (burger menu, setup sidebar) — same PickingBlocker gate the map
    // click/hover path uses (Trello #543).
    orthographic.scale = zoom_after_wheel(
        orthographic.scale,
        mouse_scroll.delta.y,
        cursor_over_ui(&blockers),
    );

    // +/- step zoom (web parity: HexMap binds '+', '=' and '-').
    if keyboard_free {
        let mut step = 0i32;
        if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
            step += 1;
        }
        if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
            step -= 1;
        }
        if step != 0 {
            let factor = if step > 0 { 0.85 } else { 1.0 / 0.85 };
            orthographic.scale = (orthographic.scale * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }

    if keyboard_free && keys.just_pressed(KeyCode::Home) {
        if let Some(bounds) = bounds.as_deref() {
            transform.translation.x = bounds.center.x;
            transform.translation.y = bounds.center.y;
        }
        orthographic.scale = DEFAULT_ZOOM;
    }

    // Vertical clamp only — horizontal travel wraps instead (see
    // `wrap_camera`).
    if let Some(bounds) = bounds.as_deref() {
        transform.translation.y = transform.translation.y.clamp(
            bounds.min.y - VERTICAL_PADDING,
            bounds.max.y + VERTICAL_PADDING,
        );
    }
}

/// Teleport the camera by one world width whenever it strays more than half
/// a width from the center copy. The ±1 wrap copies guarantee the viewport
/// is always fully covered, so the jump is invisible.
pub fn wrap_camera(
    bounds: Option<Res<MapBounds>>,
    mut camera: Query<&mut Transform, With<GameCamera>>,
) {
    let Some(bounds) = bounds else {
        return;
    };
    let Ok(mut transform) = camera.single_mut() else {
        return;
    };
    let half = bounds.width_px / 2.0;
    let dx = transform.translation.x - bounds.center.x;
    if dx > half {
        transform.translation.x -= bounds.width_px;
    } else if dx < -half {
        transform.translation.x += bounds.width_px;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_zoom_over_ui_is_ignored() {
        // Cursor over a UI panel (burger menu / setup sidebar): scrolling must
        // not change the map zoom, even with a nonzero wheel delta.
        assert_eq!(zoom_after_wheel(DEFAULT_ZOOM, 3.0, true), DEFAULT_ZOOM);
        assert_eq!(zoom_after_wheel(DEFAULT_ZOOM, -3.0, true), DEFAULT_ZOOM);
    }

    #[test]
    fn wheel_zoom_over_map_changes_scale() {
        // Over the bare map, a wheel-up zooms in (smaller scale) and a
        // wheel-down zooms out (larger scale).
        assert!(zoom_after_wheel(DEFAULT_ZOOM, 1.0, false) < DEFAULT_ZOOM);
        assert!(zoom_after_wheel(DEFAULT_ZOOM, -1.0, false) > DEFAULT_ZOOM);
    }

    #[test]
    fn wheel_zoom_clamps_to_bounds() {
        // Runaway scroll can't push scale past the configured limits.
        assert_eq!(zoom_after_wheel(MIN_ZOOM, 100.0, false), MIN_ZOOM);
        assert_eq!(zoom_after_wheel(MAX_ZOOM, -100.0, false), MAX_ZOOM);
    }

    #[test]
    fn no_wheel_delta_is_a_noop() {
        assert_eq!(zoom_after_wheel(DEFAULT_ZOOM, 0.0, false), DEFAULT_ZOOM);
    }
}
