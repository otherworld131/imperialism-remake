use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};

use crate::hex_renderer::{GameStateResource, MapBounds, hex_to_pixel};

/// Marker component for the main game camera.
#[derive(Component)]
pub struct GameCamera;

const CAMERA_SPEED: f32 = 760.0;
const MIN_ZOOM: f32 = 0.45;
const MAX_ZOOM: f32 = 2.8;
const MAP_PADDING: f32 = 420.0;

/// Spawn the 2D camera centered on the map.
pub fn setup_camera(mut commands: Commands, game: Res<GameStateResource>) {
    let map_w = game.0.world.hex_map.width();
    let map_h = game.0.world.hex_map.height();
    let center = hex_to_pixel(map_w / 2, map_h / 2);

    commands.spawn((
        Camera2d,
        Transform::from_xyz(center.x, center.y, 0.0),
        Projection::from(OrthographicProjection {
            scale: 1.25,
            ..OrthographicProjection::default_2d()
        }),
        GameCamera,
    ));
}

/// Handle keyboard pan, mouse-drag pan, and wheel zoom.
pub fn camera_movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    map_bounds: Option<Res<MapBounds>>,
    mut query: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Ok((mut transform, mut projection)) = query.single_mut() else {
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
        if let Some(bounds) = map_bounds.as_deref() {
            transform.translation.x = bounds.center.x;
            transform.translation.y = bounds.center.y;
        }
        orthographic.scale = 1.25;
    }

    if let Some(bounds) = map_bounds.as_deref() {
        transform.translation.x = transform
            .translation
            .x
            .clamp(bounds.min.x - MAP_PADDING, bounds.max.x + MAP_PADDING);
        transform.translation.y = transform
            .translation
            .y
            .clamp(bounds.min.y - MAP_PADDING, bounds.max.y + MAP_PADDING);
    }
}
