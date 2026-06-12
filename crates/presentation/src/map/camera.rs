//! 2D map camera: WASD/arrow pan, right/middle-drag pan, wheel zoom, Home
//! reset, plus the horizontal-wrap teleport that keeps the camera within
//! half a world width of the center copy.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::game::resources::CameraCentered;
use crate::map::layers::MapBounds;

#[derive(Component)]
pub struct GameCamera;

const CAMERA_SPEED: f32 = 760.0;
const MIN_ZOOM: f32 = 0.45;
const MAX_ZOOM: f32 = 2.8;
const DEFAULT_ZOOM: f32 = 1.25;
const VERTICAL_PADDING: f32 = 420.0;

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
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    bounds: Option<Res<MapBounds>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ref mut orthographic) = *projection else {
        return;
    };

    let mut direction = Vec2::ZERO;
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

    if mouse_scroll.delta.y != 0.0 {
        let zoom_factor = 1.0 - mouse_scroll.delta.y * 0.08;
        orthographic.scale = (orthographic.scale * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    if keys.just_pressed(KeyCode::Home) {
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
