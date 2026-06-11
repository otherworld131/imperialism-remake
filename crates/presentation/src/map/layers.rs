//! Merged map layers. Instead of one entity per tile, every terrain type
//! (terrain mode) and every owner (political mode) gets a single merged
//! mesh, spawned three times under the wrap roots so the map tiles
//! seamlessly across the horizontal seam.

use bevy::prelude::*;
use std::collections::BTreeMap;

use crate::game::resources::ViewModels;
use crate::map::camera::GameCamera;
use crate::map::geometry::{self, HEX_SIZE};
use crate::map::picking::{HoveredHex, SelectedHex};
use crate::theme;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapMode {
    Terrain,
    #[default]
    Political,
}

impl MapMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Political => "Political",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Terrain => Self::Political,
            Self::Political => Self::Terrain,
        }
    }
}

/// One of the three horizontal copies of the world (offsets -1, 0, +1).
#[derive(Component)]
pub struct WrapRoot;

#[derive(Component)]
pub struct TerrainLayer;

#[derive(Component)]
pub struct PoliticalLayer;

/// The [`crate::game::resources::DataVersion`] a layer entity was built
/// from; stale layers are despawned and rebuilt.
#[derive(Component)]
pub struct LayerBuiltAt(pub u64);

#[derive(Component)]
pub struct HoverRing;

#[derive(Component)]
pub struct SelectionRing;

/// World-space extent of the (unwrapped) map, derived from the map VM.
#[derive(Resource, Clone, Copy)]
pub struct MapBounds {
    pub min: Vec2,
    pub max: Vec2,
    pub center: Vec2,
    /// Horizontal wrap period in world units.
    pub width_px: f32,
    /// Wrap period in hex columns.
    pub map_width: i32,
}

pub fn setup_rings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Mesh2d(meshes.add(geometry::pointy_hex_ring_mesh(
            HEX_SIZE * 1.08,
            HEX_SIZE * 0.96,
        ))),
        MeshMaterial2d(materials.add(Color::srgba(1.0, 0.94, 0.62, 0.85))),
        Transform::from_xyz(0.0, 0.0, 5.0),
        Visibility::Hidden,
        HoverRing,
    ));
    commands.spawn((
        Mesh2d(meshes.add(geometry::pointy_hex_ring_mesh(
            HEX_SIZE * 1.13,
            HEX_SIZE * 0.98,
        ))),
        MeshMaterial2d(materials.add(Color::srgba(1.0, 1.0, 1.0, 0.95))),
        Transform::from_xyz(0.0, 0.0, 6.0),
        Visibility::Hidden,
        SelectionRing,
    ));
}

pub fn handle_map_mode_input(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<MapMode>) {
    if keys.just_pressed(KeyCode::KeyM) || keys.just_pressed(KeyCode::Tab) {
        *mode = mode.toggled();
    }
}

/// Despawn and rebuild the merged layers when the map VM was recomputed
/// for a newer data version (e.g. after end turn).
pub fn rebuild_layers(
    mut commands: Commands,
    vms: Res<ViewModels>,
    mode: Res<MapMode>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    wrap_roots: Query<Entity, With<WrapRoot>>,
    layers: Query<(Entity, &LayerBuiltAt)>,
) {
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    if vms.version == 0 || tiles.is_empty() {
        return;
    }
    let current = !layers.is_empty() && layers.iter().all(|(_, built)| built.0 == vms.version);
    if current {
        return;
    }

    for (entity, _) in &layers {
        commands.entity(entity).despawn();
    }

    // Group hex centers by fill color. BTreeMap keeps group order (and thus
    // entity order) deterministic across rebuilds.
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut terrain_groups: BTreeMap<String, Vec<Vec2>> = BTreeMap::new();
    let mut political_groups: BTreeMap<String, Vec<Vec2>> = BTreeMap::new();

    for tile in tiles {
        let pos = geometry::hex_to_world(tile.q, tile.r);
        min = min.min(pos);
        max = max.max(pos);
        terrain_groups
            .entry(tile.terrain.clone())
            .or_default()
            .push(pos);
        // Political mode: owned land takes the owner's tint; sea and
        // unowned land fall back to their terrain color.
        let political_key = if tile.terrain == "Sea" || tile.owner_color.is_empty() {
            format!("terrain:{}", tile.terrain)
        } else {
            format!("nation:{}", tile.owner_color)
        };
        political_groups.entry(political_key).or_default().push(pos);
    }

    let map_width = tiles[0].map_width;
    let bounds = MapBounds {
        min,
        max,
        center: (min + max) / 2.0,
        width_px: geometry::world_width_px(map_width),
        map_width,
    };
    commands.insert_resource(bounds);

    let roots: Vec<Entity> = if wrap_roots.is_empty() {
        [-1.0f32, 0.0, 1.0]
            .into_iter()
            .map(|offset| {
                commands
                    .spawn((
                        WrapRoot,
                        Transform::from_xyz(offset * bounds.width_px, 0.0, 0.0),
                        Visibility::default(),
                    ))
                    .id()
            })
            .collect()
    } else {
        wrap_roots.iter().collect()
    };

    // Slight overdraw hides hairline seams between adjacent hexes.
    let radius = HEX_SIZE + 0.45;
    let version = vms.version;

    let terrain_visibility = match *mode {
        MapMode::Terrain => Visibility::Inherited,
        MapMode::Political => Visibility::Hidden,
    };

    let mut spawn_group = |centers: &[Vec2], color: Color, political: bool| {
        // Mesh and material are shared by all three wrap copies.
        let mesh = meshes.add(geometry::merged_hex_mesh(centers, radius));
        let material = materials.add(color);
        for &root in &roots {
            let mut entity = commands.spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0),
                LayerBuiltAt(version),
                if political {
                    invert(terrain_visibility)
                } else {
                    terrain_visibility
                },
                ChildOf(root),
            ));
            if political {
                entity.insert(PoliticalLayer);
            } else {
                entity.insert(TerrainLayer);
            }
        }
    };

    for (terrain, centers) in &terrain_groups {
        spawn_group(centers, theme::terrain_color(terrain), false);
    }
    for (key, centers) in &political_groups {
        let color = match key.split_once(':') {
            Some(("nation", name)) => theme::political_tint(theme::nation_color(name)),
            Some((_, terrain)) => theme::terrain_color(terrain),
            None => theme::terrain_color(key),
        };
        spawn_group(centers, color, true);
    }
}

fn invert(visibility: Visibility) -> Visibility {
    match visibility {
        Visibility::Hidden => Visibility::Inherited,
        _ => Visibility::Hidden,
    }
}

/// Flip layer visibility when the map mode toggles.
pub fn apply_map_mode(
    mode: Res<MapMode>,
    mut layer_queries: ParamSet<(
        Query<&mut Visibility, With<TerrainLayer>>,
        Query<&mut Visibility, With<PoliticalLayer>>,
    )>,
) {
    if !mode.is_changed() {
        return;
    }
    let terrain_visible = matches!(*mode, MapMode::Terrain);
    for mut visibility in &mut layer_queries.p0() {
        *visibility = if terrain_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for mut visibility in &mut layer_queries.p1() {
        *visibility = if terrain_visible {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// Move the hover/selection rings to their hexes, picking the wrap copy
/// closest to the camera so the marker never sits a full world away.
pub fn update_rings(
    hovered: Res<HoveredHex>,
    selected: Res<SelectedHex>,
    bounds: Option<Res<MapBounds>>,
    camera: Query<&Transform, (With<GameCamera>, Without<HoverRing>, Without<SelectionRing>)>,
    mut rings: ParamSet<(
        Query<(&mut Transform, &mut Visibility), With<HoverRing>>,
        Query<(&mut Transform, &mut Visibility), With<SelectionRing>>,
    )>,
) {
    let camera_x = camera.single().map(|t| t.translation.x).unwrap_or(0.0);
    let bounds = bounds.as_deref().copied();
    place_ring(&mut rings.p0(), hovered.0, camera_x, bounds);
    place_ring(&mut rings.p1(), selected.0, camera_x, bounds);
}

fn place_ring<F: bevy::ecs::query::QueryFilter>(
    query: &mut Query<(&mut Transform, &mut Visibility), F>,
    coord: Option<(i32, i32)>,
    camera_x: f32,
    bounds: Option<MapBounds>,
) {
    let Ok((mut transform, mut visibility)) = query.single_mut() else {
        return;
    };
    match coord {
        Some((q, r)) => {
            let mut pos = geometry::hex_to_world(q, r);
            if let Some(bounds) = bounds {
                pos.x += ((camera_x - pos.x) / bounds.width_px).round() * bounds.width_px;
            }
            transform.translation.x = pos.x;
            transform.translation.y = pos.y;
            *visibility = Visibility::Visible;
        }
        None => *visibility = Visibility::Hidden,
    }
}
