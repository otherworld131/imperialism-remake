use application::{GameState, HexCoord, ProvinceId, TerrainType};
use bevy::prelude::*;
use std::collections::HashMap;

use crate::colors;

/// Size of hex tiles in pixels (outer radius).
pub const HEX_SIZE: f32 = 24.0;
const SQRT_3: f32 = 1.732_050_8;
const HEX_ROTATION: f32 = -std::f32::consts::FRAC_PI_6;

#[derive(Component)]
pub struct HexTileVisual {
    terrain_material: Handle<ColorMaterial>,
    political_material: Handle<ColorMaterial>,
}

#[derive(Component)]
pub struct HoverMarker;

#[derive(Component)]
pub struct SelectionMarker;

#[derive(Resource, Default)]
pub struct HoveredTile {
    pub coord: Option<HexCoord>,
}

#[derive(Resource, Default)]
pub struct SelectedTile {
    pub coord: Option<HexCoord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategicMapMode {
    Terrain,
    Political,
}

impl StrategicMapMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Political => "Political",
        }
    }
}

#[derive(Resource)]
pub struct MapModeState {
    pub mode: StrategicMapMode,
}

impl Default for MapModeState {
    fn default() -> Self {
        Self {
            mode: StrategicMapMode::Political,
        }
    }
}

#[derive(Resource)]
pub struct MapBounds {
    pub min: Vec2,
    pub max: Vec2,
    pub center: Vec2,
}

/// Bevy resource wrapping the game state.
#[derive(Resource)]
pub struct GameStateResource(pub GameState);

/// Convert hex axial coordinates to pixel position (pointy-top).
pub fn hex_to_pixel(q: i32, r: i32) -> Vec2 {
    let x = HEX_SIZE * (SQRT_3 * q as f32 + SQRT_3 / 2.0 * r as f32);
    let y = HEX_SIZE * (3.0 / 2.0 * r as f32);
    Vec2::new(x, -y)
}

pub fn render_hex_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    game: Res<GameStateResource>,
) {
    let fill_mesh = meshes.add(RegularPolygon::new(HEX_SIZE * 0.97, 6));
    let hover_mesh = meshes.add(RegularPolygon::new(HEX_SIZE * 1.03, 6).to_ring(3.0));
    let selection_mesh = meshes.add(RegularPolygon::new(HEX_SIZE * 1.08, 6).to_ring(3.8));

    let hover_material = materials.add(Color::srgba(1.0, 0.94, 0.62, 0.85));
    let selection_material = materials.add(Color::srgba(1.0, 1.0, 1.0, 0.95));
    let capital_material = materials.add(Color::srgb(0.08, 0.08, 0.08));
    let marker_material = materials.add(Color::srgba(0.08, 0.08, 0.08, 0.82));

    let game = &game.0;
    let province_materials = province_owner_materials(game, &mut materials);
    let terrain_materials = terrain_materials(&mut materials);

    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);

    for (coord, tile) in game.world.hex_map.all_tiles() {
        let pos = hex_to_pixel(coord.q, coord.r);
        min.x = min.x.min(pos.x);
        min.y = min.y.min(pos.y);
        max.x = max.x.max(pos.x);
        max.y = max.y.max(pos.y);

        let terrain_material = terrain_materials
            .get(&tile.terrain())
            .expect("all terrain types have a material")
            .clone();
        let political_material = if tile.terrain() == TerrainType::Sea {
            terrain_materials
                .get(&TerrainType::Sea)
                .expect("sea has a material")
                .clone()
        } else {
            tile.province_id
                .and_then(|pid| province_materials.get(&pid).cloned())
                .unwrap_or_else(|| terrain_material.clone())
        };

        commands.spawn((
            Mesh2d(fill_mesh.clone()),
            MeshMaterial2d(political_material.clone()),
            Transform::from_xyz(pos.x, pos.y, 0.0)
                .with_rotation(Quat::from_rotation_z(HEX_ROTATION)),
            HexTileVisual {
                terrain_material,
                political_material,
            },
        ));

        if tile.is_country_capital {
            commands.spawn((
                Text2d::new("CAP"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Center),
                Transform::from_xyz(pos.x, pos.y + 1.0, 2.0),
            ));
            commands.spawn((
                Mesh2d(hover_mesh.clone()),
                MeshMaterial2d(capital_material.clone()),
                Transform::from_xyz(pos.x, pos.y, 1.0)
                    .with_rotation(Quat::from_rotation_z(HEX_ROTATION)),
            ));
        } else if let Some(label) = resource_label(tile.resource_deposit()) {
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgba(0.04, 0.04, 0.04, 0.86)),
                TextLayout::new_with_justify(Justify::Center),
                Transform::from_xyz(pos.x, pos.y - 1.0, 1.5),
            ));
        }

        let mut infra = String::new();
        if tile.infrastructure.has_railroad {
            infra.push('R');
        }
        if tile.infrastructure.has_depot {
            infra.push('D');
        }
        if tile.infrastructure.has_port {
            infra.push('P');
        }
        if tile.infrastructure.has_fort {
            infra.push('F');
        }
        if !infra.is_empty() {
            commands.spawn((
                Text2d::new(infra),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Center),
                Transform::from_xyz(pos.x, pos.y - 10.0, 2.0),
            ));
            commands.spawn((
                Mesh2d(meshes.add(Circle::new(8.0))),
                MeshMaterial2d(marker_material.clone()),
                Transform::from_xyz(pos.x, pos.y - 10.0, 1.8),
            ));
        }
    }

    let center = (min + max) / 2.0;
    commands.insert_resource(MapBounds { min, max, center });

    commands.spawn((
        Mesh2d(hover_mesh),
        MeshMaterial2d(hover_material),
        Transform::from_xyz(center.x, center.y, 5.0)
            .with_rotation(Quat::from_rotation_z(HEX_ROTATION)),
        Visibility::Hidden,
        HoverMarker,
    ));

    commands.spawn((
        Mesh2d(selection_mesh),
        MeshMaterial2d(selection_material),
        Transform::from_xyz(center.x, center.y, 6.0)
            .with_rotation(Quat::from_rotation_z(HEX_ROTATION)),
        Visibility::Hidden,
        SelectionMarker,
    ));
}

pub fn handle_map_mode_input(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<MapModeState>) {
    if keys.just_pressed(KeyCode::KeyM) || keys.just_pressed(KeyCode::Tab) {
        state.mode = match state.mode {
            StrategicMapMode::Terrain => StrategicMapMode::Political,
            StrategicMapMode::Political => StrategicMapMode::Terrain,
        };
    }
}

pub fn update_tile_materials(
    mode: Res<MapModeState>,
    mut tiles: Query<(&HexTileVisual, &mut MeshMaterial2d<ColorMaterial>)>,
) {
    if !mode.is_changed() {
        return;
    }

    for (visual, mut material) in &mut tiles {
        material.0 = match mode.mode {
            StrategicMapMode::Terrain => visual.terrain_material.clone(),
            StrategicMapMode::Political => visual.political_material.clone(),
        };
    }
}

pub fn update_tile_highlights(
    hovered: Res<HoveredTile>,
    selected: Res<SelectedTile>,
    mut highlight_queries: ParamSet<(
        Query<(&mut Transform, &mut Visibility), With<HoverMarker>>,
        Query<(&mut Transform, &mut Visibility), With<SelectionMarker>>,
    )>,
) {
    if hovered.is_changed() {
        if let Ok((mut transform, mut visibility)) = highlight_queries.p0().single_mut() {
            if let Some(coord) = hovered.coord {
                let pos = hex_to_pixel(coord.q, coord.r);
                transform.translation.x = pos.x;
                transform.translation.y = pos.y;
                *visibility = Visibility::Visible;
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }

    if selected.is_changed() {
        if let Ok((mut transform, mut visibility)) = highlight_queries.p1().single_mut() {
            if let Some(coord) = selected.coord {
                let pos = hex_to_pixel(coord.q, coord.r);
                transform.translation.x = pos.x;
                transform.translation.y = pos.y;
                *visibility = Visibility::Visible;
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

fn terrain_materials(
    materials: &mut Assets<ColorMaterial>,
) -> HashMap<TerrainType, Handle<ColorMaterial>> {
    [
        TerrainType::Grassland,
        TerrainType::Hills,
        TerrainType::Forest,
        TerrainType::Mountain,
        TerrainType::Desert,
        TerrainType::Swamp,
        TerrainType::Tundra,
        TerrainType::Sea,
    ]
    .into_iter()
    .map(|terrain| (terrain, materials.add(colors::terrain_color(terrain))))
    .collect()
}

fn province_owner_materials(
    game: &GameState,
    materials: &mut Assets<ColorMaterial>,
) -> HashMap<ProvinceId, Handle<ColorMaterial>> {
    let mut materials_by_province = HashMap::new();
    for nation in &game.world.nations {
        let material = materials.add(colors::nation_color(nation.color));
        for pid in &nation.province_ids {
            materials_by_province.insert(*pid, material.clone());
        }
    }
    materials_by_province
}

fn resource_label(resource: Option<application::ResourceType>) -> Option<String> {
    resource.map(|resource| {
        let name = format!("{resource:?}");
        name.chars().next().unwrap_or('?').to_string()
    })
}
