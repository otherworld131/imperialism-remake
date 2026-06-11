//! Cursor → hex picking. Converts the cursor through the camera into world
//! space, inverts the axial transform, and wrap-normalizes the column so
//! picking works on every wrap copy. HUD nodes tagged [`PickingBlocker`]
//! suppress picking while hovered.

use bevy::prelude::*;

use crate::game::resources::TileIndex;
use crate::map::camera::GameCamera;
use crate::map::geometry;
use crate::map::layers::MapBounds;

#[derive(Resource, Default)]
pub struct HoveredHex(pub Option<(i32, i32)>);

#[derive(Resource, Default)]
pub struct SelectedHex(pub Option<(i32, i32)>);

/// Marker for HUD nodes that swallow map picking while the cursor is over
/// them. Must be paired with an `Interaction` component.
#[derive(Component)]
pub struct PickingBlocker;

fn cursor_over_ui(blockers: &Query<&Interaction, With<PickingBlocker>>) -> bool {
    blockers
        .iter()
        .any(|interaction| *interaction != Interaction::None)
}

pub fn pick_hover(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    bounds: Option<Res<MapBounds>>,
    index: Res<TileIndex>,
    blockers: Query<&Interaction, With<PickingBlocker>>,
    mut hovered: ResMut<HoveredHex>,
) {
    let coord = if cursor_over_ui(&blockers) {
        None
    } else {
        cursor_hex(&windows, &camera, bounds.as_deref(), &index)
    };
    if hovered.0 != coord {
        hovered.0 = coord;
    }
}

pub fn pick_select(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    blockers: Query<&Interaction, With<PickingBlocker>>,
    hovered: Res<HoveredHex>,
    mut selected: ResMut<SelectedHex>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) || cursor_over_ui(&blockers) {
        return;
    }
    if selected.0 != hovered.0 {
        selected.0 = hovered.0;
    }
}

fn cursor_hex(
    windows: &Query<&Window>,
    camera: &Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    bounds: Option<&MapBounds>,
    index: &TileIndex,
) -> Option<(i32, i32)> {
    let window = windows.single().ok()?;
    let (camera, camera_transform) = camera.single().ok()?;
    let cursor = window.cursor_position()?;
    let world = camera.viewport_to_world_2d(camera_transform, cursor).ok()?;
    let bounds = bounds?;
    let (q, r) = geometry::world_to_hex(world);
    // The cursor may sit on a wrap copy; the stored tile is the same column
    // shifted by a multiple of the map width.
    (-2..=2)
        .map(|k| (q + k * bounds.map_width, r))
        .find(|coord| index.by_coord.contains_key(coord))
}
