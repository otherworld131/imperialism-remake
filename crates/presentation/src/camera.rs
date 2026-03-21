use bevy::prelude::*;

/// Marker component for the main game camera.
#[derive(Component)]
pub struct GameCamera;

/// Resource tracking the current zoom level for the camera.
#[derive(Resource)]
pub struct CameraZoom {
    pub scale: f32,
}

impl Default for CameraZoom {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// Camera movement speed.
const CAMERA_SPEED: f32 = 500.0;
const ZOOM_SPEED: f32 = 0.1;
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 3.0;

/// Spawn the 2D camera.
pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Transform::from_xyz(600.0, -400.0, 0.0),
        GameCamera,
    ));
    commands.init_resource::<CameraZoom>();
}

/// Handle camera pan (WASD/arrows) and zoom (Q/E or scroll).
pub fn camera_movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut zoom: ResMut<CameraZoom>,
    mut query: Query<&mut Transform, With<GameCamera>>,
) {
    let Ok(mut transform) = query.single_mut() else {
        return;
    };

    let dt = time.delta_secs();
    let speed = CAMERA_SPEED * zoom.scale;

    // Pan
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

    // Zoom
    if keys.pressed(KeyCode::KeyQ) || keys.pressed(KeyCode::Minus) {
        zoom.scale = (zoom.scale + ZOOM_SPEED).min(MAX_ZOOM);
    }
    if keys.pressed(KeyCode::KeyE) || keys.pressed(KeyCode::Equal) {
        zoom.scale = (zoom.scale - ZOOM_SPEED).max(MIN_ZOOM);
    }

    transform.scale = Vec3::splat(zoom.scale);
}
