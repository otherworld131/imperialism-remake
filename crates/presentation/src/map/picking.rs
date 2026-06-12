//! Cursor → map picking with the web frontend's hit order: navy marker →
//! pending-treaty marker → troop indicator → hex. Converts the cursor
//! through the camera into world space, wrap-normalizes the position, and
//! runs radius tests in world space. HUD nodes tagged [`PickingBlocker`]
//! suppress picking while hovered.

use bevy::prelude::*;

use crate::game::resources::{TreatyMarkerIndex, ViewModels};
use crate::game::vm::NavyMarker;
use crate::map::camera::GameCamera;
use crate::map::geometry;
use crate::map::layers::MapBounds;
use crate::map::lod::ZoomLod;
use crate::map::markers::troop_indicator_hit;
use crate::map::navy;
use crate::state::Screen;

#[derive(Resource, Default)]
pub struct HoveredHex(pub Option<(i32, i32)>);

#[derive(Resource, Default)]
pub struct SelectedHex(pub Option<(i32, i32)>);

/// What the cursor is over, in hit-order priority. Drives the map tooltip.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub enum HoverTarget {
    #[default]
    None,
    Hex(i32, i32),
    /// Stable navy-marker key (see [`navy::marker_key`]).
    Navy(String),
    /// Pending diplomacy marker under a nation label — clicking dismisses
    /// the queued action. Only produced on the Diplomacy screen.
    Treaty {
        nation_id: u32,
        action_key: String,
    },
}

/// Marker for HUD nodes that swallow map picking while the cursor is over
/// them. Must be paired with an `Interaction` component.
#[derive(Component)]
pub struct PickingBlocker;

/// A left-click on the map, in hit order. Consumed by
/// `game::selection::handle_map_click`, which owns the web frontend's
/// click-priority logic (fleet move → unit move → deploy → selection).
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct MapClick(pub HoverTarget);

fn cursor_over_ui(blockers: &Query<&Interaction, With<PickingBlocker>>) -> bool {
    blockers
        .iter()
        .any(|interaction| *interaction != Interaction::None)
}

/// Cursor world position, wrap-normalized into the primary map copy.
fn cursor_world(
    windows: &Query<&Window>,
    camera: &Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    bounds: &MapBounds,
) -> Option<Vec2> {
    let window = windows.single().ok()?;
    let (camera, camera_transform) = camera.single().ok()?;
    let cursor = window.cursor_position()?;
    let mut world = camera.viewport_to_world_2d(camera_transform, cursor).ok()?;
    // Wrap x into [0, width) like the web's wrapWorldX.
    world.x = world.x.rem_euclid(bounds.width_px);
    Some(world)
}

fn navy_marker_at(markers: &[NavyMarker], world: Vec2) -> Option<&NavyMarker> {
    let r2 = navy::NAVY_MARKER_RADIUS * navy::NAVY_MARKER_RADIUS;
    let indices = navy::anchor_index_map(markers);
    // Reverse order so markers drawn last (on top) hit-test first.
    markers.iter().rev().find(|m| {
        let index = indices.get(&navy::marker_key(m)).copied().unwrap_or(0);
        let (dx, dy) = navy::marker_offset(index);
        let pos = geometry::hex_to_world(m.q, m.r) + Vec2::new(dx, -dy);
        pos.distance_squared(world) <= r2
    })
}

pub fn pick_hover(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    bounds: Option<Res<MapBounds>>,
    vms: Res<ViewModels>,
    index: Res<crate::game::resources::TileIndex>,
    lod: Res<ZoomLod>,
    screen: Res<State<Screen>>,
    treaty_markers: Res<TreatyMarkerIndex>,
    blockers: Query<&Interaction, With<PickingBlocker>>,
    mut hovered: ResMut<HoveredHex>,
    mut target: ResMut<HoverTarget>,
) {
    let mut next_hex = None;
    let mut next_target = HoverTarget::None;
    if !cursor_over_ui(&blockers)
        && let Some(bounds) = bounds.as_deref()
        && let Some(world) = cursor_world(&windows, &camera, bounds)
    {
        // Pending-treaty markers are only interactive on the Diplomacy
        // screen (web: `onPendingTreatyMarkerClick` gated on the screen).
        let treaty_hit = (*screen.get() == Screen::Diplomacy)
            .then(|| {
                treaty_markers.0.iter().find(|m| {
                    m.action_key.is_some() && m.pos.distance_squared(world) <= m.radius * m.radius
                })
            })
            .flatten();
        if let Some(marker) = navy_marker_at(&vms.navy_markers, world) {
            next_target = HoverTarget::Navy(navy::marker_key(marker));
        } else if let Some(hit) = treaty_hit {
            next_target = HoverTarget::Treaty {
                nation_id: hit.nation_id,
                action_key: hit.action_key.clone().unwrap_or_default(),
            };
        } else {
            // The troop indicator only exists past its LOD gate; below it,
            // hits fall through to the hex like the web (scale > 0.6 gate).
            let troop_hit = if lod.troops {
                vms.map
                    .as_ref()
                    .and_then(|tiles| troop_indicator_hit(tiles, world))
            } else {
                None
            };
            let coord = troop_hit.or_else(|| {
                let (q, r) = geometry::world_to_hex(world);
                (-2..=2)
                    .map(|k| (q + k * bounds.map_width, r))
                    .find(|coord| index.by_coord.contains_key(coord))
            });
            if let Some((q, r)) = coord {
                next_hex = Some((q, r));
                next_target = HoverTarget::Hex(q, r);
            }
        }
    }
    if hovered.0 != next_hex {
        hovered.0 = next_hex;
    }
    if *target != next_target {
        *target = next_target;
    }
}

pub fn pick_select(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    blockers: Query<&Interaction, With<PickingBlocker>>,
    target: Res<HoverTarget>,
    mut clicks: MessageWriter<MapClick>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) || cursor_over_ui(&blockers) {
        return;
    }
    clicks.write(MapClick(target.clone()));
}
