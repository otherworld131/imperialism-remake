use bevy::prelude::*;

use crate::hex_renderer::{GameStateResource, hex_to_pixel};

/// Marker component for the main game camera.
#[derive(Component)]
pub struct GameCamera;

/// Camera movement speed.
const CAMERA_SPEED: f32 = 300.0;

/// Spawn the 2D camera centered on the map.
pub fn setup_camera(mut commands: Commands, game: Res<GameStateResource>) {
    let map_w = game.0.hex_map.width();
    let map_h = game.0.hex_map.height();
    let center = hex_to_pixel(map_w / 2, map_h / 2);

    println!("Camera spawning at: ({}, {})", center.x, center.y);

    commands.spawn((
        Camera2d,
        Transform::from_xyz(center.x, center.y, 0.0),
        GameCamera,
    ));
}

/// Handle camera pan (WASD/arrows). No zoom for now to keep things simple.
pub fn camera_movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Transform, With<GameCamera>>,
) {
    let Ok(mut transform) = query.single_mut() else {
        return;
    };

    let dt = time.delta_secs();
    let speed = CAMERA_SPEED;

    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        transform.translation.y += speed * dt;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        transform.translation.y -= speed * dt;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        transform.translation.x -= speed * dt;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        transform.translation.x += speed * dt;
    }

    // Home key — recenter on map
    if keys.just_pressed(KeyCode::Home) {
        // Reset to origin for debugging
        transform.translation.x = 0.0;
        transform.translation.y = 0.0;
        println!("Camera reset to (0, 0)");
    }
}
