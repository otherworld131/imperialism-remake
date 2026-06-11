//! Merged map mesh layers: tile fills for all six map modes, organic
//! fill-correction strips, fog of war, sea zones, the hex grid, border
//! strokes, rivers, and railroads. Everything is merged per color and
//! spawned three times under the wrap roots so the map tiles seamlessly
//! across the horizontal seam.
//!
//! Styling constants mirror `web/src/components/HexMap.tsx`; React sizes are
//! authored against hex 18 and scaled by `REACT_SCALE` here.

use bevy::prelude::*;
use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::game::resources::{FleetTargets, MoveTargets, RenderSettings, ViewModels};
use crate::game::vm::MapTile;
use crate::map::borders::{self, MapBorders};
use crate::map::camera::GameCamera;
use crate::map::geometry::{self, HEX_SIZE};
use crate::map::lod::LodGate;
use crate::map::organic::Point;
use crate::map::picking::{HoveredHex, SelectedHex};
use crate::map::polyline::MeshBuilder2d;
use crate::theme;

/// World units per React canvas unit (hex 24 vs hex 18).
pub const REACT_SCALE: f32 = HEX_SIZE / 18.0;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MapMode {
    Terrain,
    #[default]
    Political,
    Diplomatic,
    Relationship,
    Military,
    Naval,
}

impl MapMode {
    pub const ALL: [MapMode; 6] = [
        Self::Terrain,
        Self::Political,
        Self::Diplomatic,
        Self::Relationship,
        Self::Military,
        Self::Naval,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Political => "Political",
            Self::Diplomatic => "Diplomatic",
            Self::Relationship => "Relationship",
            Self::Military => "Military",
            Self::Naval => "Naval",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    pub fn cycled(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    /// Overlay modes recolor nation fills from diplomacy/military data.
    pub fn is_overlay(self) -> bool {
        matches!(
            self,
            Self::Diplomatic | Self::Relationship | Self::Military | Self::Naval
        )
    }
}

/// One of the three horizontal copies of the world (offsets -1, 0, +1).
#[derive(Component)]
pub struct WrapRoot;

/// Everything (re)built by [`rebuild_layers`].
#[derive(Component)]
pub struct StaticLayer;

/// Everything (re)built by [`rebuild_highlight_layers`].
#[derive(Component)]
pub struct HighlightLayer;

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

/// Border geometry cache; recomputed only when the map version moves.
#[derive(Resource, Default)]
pub struct BordersCache {
    pub version: u64,
    pub data: Option<MapBorders>,
}

/// React pixel point (y down) → world point (y up).
pub fn react_to_world(p: Point) -> Vec2 {
    Vec2::new(p[0] as f32, -(p[1] as f32))
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
        *mode = mode.cycled();
    }
}

// ── Fill colors ─────────────────────────────────────────────────────────

/// nation_name → overlay fill for diplomatic/relationship/military/naval
/// modes, mirroring `nationFillMap` in HexMap.tsx.
pub fn nation_fill_map(mode: MapMode, vms: &ViewModels) -> HashMap<String, Color> {
    let mut map = HashMap::new();
    match mode {
        MapMode::Diplomatic | MapMode::Relationship => {
            if let Some(overlay) = vms.diplomacy.as_ref() {
                map.insert(overlay.selected_nation.clone(), theme::OVERLAY_SELF);
                for rel in &overlay.relations {
                    let color = if mode == MapMode::Diplomatic {
                        theme::diplo_status_color(&rel.status)
                    } else {
                        theme::score_color(rel.score as f32)
                    };
                    map.insert(rel.nation_name.clone(), color);
                }
            }
        }
        MapMode::Military | MapMode::Naval => {
            if !vms.military.is_empty() {
                let values: Vec<f64> = vms
                    .military
                    .iter()
                    .map(|e| {
                        if mode == MapMode::Military {
                            e.total_army_fp
                        } else {
                            e.total_naval_fp
                        }
                    })
                    .collect();
                let avg = values.iter().sum::<f64>() / values.len().max(1) as f64;
                let max_dev = values
                    .iter()
                    .map(|v| (v - avg).abs())
                    .fold(1.0_f64, f64::max);
                for (entry, value) in vms.military.iter().zip(&values) {
                    let score = ((value - avg) / max_dev * 100.0).round() as f32;
                    map.insert(entry.nation_name.clone(), theme::strength_color(score));
                }
            }
        }
        _ => {}
    }
    map
}

fn political_color(tile: &MapTile) -> Color {
    if tile.owner_color.is_empty() {
        return theme::terrain_color(&tile.terrain);
    }
    let nation = theme::nation_color(&tile.owner_color);
    if tile.is_incorporated_minor {
        theme::incorporated_tint(nation)
    } else {
        theme::political_tint(nation)
    }
}

/// Fill color for one tile in the current map mode (`tileFillColor`).
pub fn tile_fill_color(tile: &MapTile, mode: MapMode, fill_map: &HashMap<String, Color>) -> Color {
    if tile.is_sea() {
        return theme::terrain_color("Sea");
    }
    match mode {
        MapMode::Terrain => {
            let base = theme::terrain_color(&tile.terrain);
            if tile.owner_color.is_empty() {
                base
            } else {
                let amount = if tile.is_incorporated_minor {
                    0.10
                } else {
                    0.15
                };
                theme::terrain_nation_tint(base, theme::nation_color(&tile.owner_color), amount)
            }
        }
        MapMode::Political => political_color(tile),
        _ => {
            if !tile.owner.is_empty()
                && let Some(color) = fill_map.get(&tile.owner)
            {
                *color
            } else {
                political_color(tile)
            }
        }
    }
}

fn color_key(color: Color) -> u32 {
    let c = color.to_srgba().to_u8_array();
    u32::from_be_bytes(c)
}

// ── Rebuild ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub struct StaticKey {
    version: u64,
    mode: MapMode,
    settings: RenderSettings,
}

pub fn rebuild_layers(
    mut commands: Commands,
    vms: Res<ViewModels>,
    mode: Res<MapMode>,
    settings: Res<RenderSettings>,
    mut cache: ResMut<BordersCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    wrap_roots: Query<Entity, With<WrapRoot>>,
    layers: Query<Entity, With<StaticLayer>>,
    mut built: Local<Option<StaticKey>>,
) {
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    if vms.version == 0 || tiles.is_empty() {
        return;
    }
    let key = StaticKey {
        version: vms.version,
        mode: *mode,
        settings: *settings,
    };
    if built.as_ref() == Some(&key) {
        return;
    }
    *built = Some(key);

    for entity in &layers {
        commands.entity(entity).despawn();
    }

    // ── Bounds + wrap roots ─────────────────────────────────────────────
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for tile in tiles {
        let pos = geometry::hex_to_world(tile.q, tile.r);
        min = min.min(pos);
        max = max.max(pos);
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

    if cache.version != vms.version || cache.data.is_none() {
        cache.data = Some(borders::classify(tiles, f64::from(HEX_SIZE)));
        cache.version = vms.version;
    }
    let Some(map_borders) = cache.data.as_ref() else {
        return;
    };

    // Shared spawner: one mesh+material, three wrap copies.
    let spawn_mesh = |commands: &mut Commands,
                      meshes: &mut Assets<Mesh>,
                      materials: &mut Assets<ColorMaterial>,
                      mesh: Mesh,
                      color: Color,
                      z: f32,
                      gate: Option<LodGate>| {
        let mesh = meshes.add(mesh);
        let material = materials.add(color);
        for &root in &roots {
            let mut entity = commands.spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(0.0, 0.0, z),
                StaticLayer,
                ChildOf(root),
            ));
            if let Some(gate) = gate {
                entity.insert(gate);
            }
        }
    };

    let fill_map = nation_fill_map(*mode, &vms);

    // ── Pass 1: tile fills, grouped per color ───────────────────────────
    let mut fill_groups: BTreeMap<u32, (Color, Vec<Vec2>)> = BTreeMap::new();
    for tile in tiles {
        let color = tile_fill_color(tile, *mode, &fill_map);
        fill_groups
            .entry(color_key(color))
            .or_insert_with(|| (color, Vec::new()))
            .1
            .push(geometry::hex_to_world(tile.q, tile.r));
    }
    // Slight overdraw hides hairline seams between adjacent hexes.
    let radius = HEX_SIZE + 0.45;
    for (_, (color, centers)) in fill_groups {
        spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            geometry::merged_hex_mesh(&centers, radius),
            color,
            0.0,
            None,
        );
    }

    // ── Pass 1b: organic fill-correction strips ─────────────────────────
    // For every coast / country edge, the smoothed curve wanders around the
    // straight hex edge. Cover each excursion with the color of the side it
    // pokes into: tile color past the baseline outward, sea/neighbor color
    // inward. Result: fills meet exactly on the smoothed border like the
    // web's clipped-canvas rendering.
    if settings.organic_borders {
        let mut strip_builders: BTreeMap<u32, (Color, MeshBuilder2d)> = BTreeMap::new();
        let sea_color = theme::terrain_color("Sea");
        let mut add_strips = |strips: &[borders::FillStrip]| {
            for strip in strips {
                let tile_color = tile_fill_color(&tiles[strip.tile_idx], *mode, &fill_map);
                let other_color = match strip.neighbor_idx {
                    Some(ni) => tile_fill_color(&tiles[ni], *mode, &fill_map),
                    None => sea_color,
                };
                if tile_color == other_color {
                    continue;
                }
                let n = Vec2::new(strip.normal[0] as f32, strip.normal[1] as f32);
                let sign = strip.outward_sign as f32;
                let a = Vec2::new(strip.base_a[0] as f32, strip.base_a[1] as f32);
                let mut base_rail: Vec<Vec2> = Vec::with_capacity(strip.curve.len());
                let mut out_rail: Vec<Vec2> = Vec::with_capacity(strip.curve.len());
                let mut in_rail: Vec<Vec2> = Vec::with_capacity(strip.curve.len());
                for p in &strip.curve {
                    let p = Vec2::new(p[0] as f32, p[1] as f32);
                    let d = (p - a).dot(n);
                    let base = p - n * d;
                    let d_out = d * sign;
                    out_rail.push(base + n * (sign * d_out.max(0.0)));
                    in_rail.push(base + n * (sign * d_out.min(0.0)));
                    base_rail.push(base);
                }
                // y-flip into world space.
                for rail in [&mut base_rail, &mut out_rail, &mut in_rail] {
                    for p in rail.iter_mut() {
                        p.y = -p.y;
                    }
                }
                strip_builders
                    .entry(color_key(tile_color))
                    .or_insert_with(|| (tile_color, MeshBuilder2d::default()))
                    .1
                    .add_ribbon(&base_rail, &out_rail);
                strip_builders
                    .entry(color_key(other_color))
                    .or_insert_with(|| (other_color, MeshBuilder2d::default()))
                    .1
                    .add_ribbon(&base_rail, &in_rail);
            }
        };
        add_strips(&map_borders.coast_strips);
        add_strips(&map_borders.country_strips);
        for (_, (color, builder)) in strip_builders {
            if !builder.is_empty() {
                spawn_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    builder.build(),
                    color,
                    0.2,
                    None,
                );
            }
        }
    }

    // ── Pass 1c: fog of war ─────────────────────────────────────────────
    if !settings.disable_fog {
        let fogged: Vec<Vec2> = tiles
            .iter()
            .filter(|t| !t.visible)
            .map(|t| geometry::hex_to_world(t.q, t.r))
            .collect();
        if !fogged.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                geometry::merged_hex_mesh(&fogged, radius),
                Color::srgba(0.5, 0.5, 0.5, 0.35),
                0.4,
                None,
            );
        }
    }

    // ── Pass 1.5: sea zones (fill + inter-zone borders) ─────────────────
    if !vms.sea_zones.is_empty() {
        let mut zone_of: HashMap<(i32, i32), u32> = HashMap::new();
        for zone in &vms.sea_zones {
            for hex in &zone.hexes {
                zone_of.insert((hex.q, hex.r), zone.id);
            }
        }
        let centers: Vec<Vec2> = vms
            .sea_zones
            .iter()
            .flat_map(|z| z.hexes.iter().map(|h| geometry::hex_to_world(h.q, h.r)))
            .collect();
        spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            geometry::merged_hex_mesh(&centers, HEX_SIZE),
            Color::srgba(20.0 / 255.0, 70.0 / 255.0, 130.0 / 255.0, 0.12),
            0.5,
            None,
        );

        let verts = borders::hex_vertices(f64::from(HEX_SIZE));
        let mut builder = MeshBuilder2d::default();
        for zone in &vms.sea_zones {
            for hex in &zone.hexes {
                let [px, py] = borders::hex_to_pixel(hex.q, hex.r, f64::from(HEX_SIZE));
                let neighbors = borders::hex_neighbors(hex.q, hex.r);
                for (d, (nq, nr)) in neighbors.iter().enumerate() {
                    match zone_of.get(&(*nq, *nr)) {
                        // Draw once per zone pair (lower id owns the edge).
                        Some(nz) if *nz != zone.id && zone.id < *nz => {}
                        _ => continue,
                    }
                    let v1 = verts[d];
                    let v2 = verts[(d + 1) % 6];
                    builder.add_segment(
                        react_to_world([px + v1[0], py + v1[1]]),
                        react_to_world([px + v2[0], py + v2[1]]),
                        1.5 * REACT_SCALE,
                    );
                }
            }
        }
        if !builder.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                builder.build(),
                Color::srgba(0.0, 40.0 / 255.0, 100.0 / 255.0, 0.45),
                0.55,
                None,
            );
        }
    }

    // ── Pass 2: hex grid ────────────────────────────────────────────────
    if !settings.hide_hex_grid {
        let mut builder = MeshBuilder2d::default();
        for [a, b] in &map_borders.grid_segments {
            builder.add_segment(react_to_world(*a), react_to_world(*b), 0.5 * REACT_SCALE);
        }
        if !builder.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                builder.build(),
                Color::srgba(0.0, 0.0, 0.0, 0.08),
                0.6,
                Some(LodGate::Grid),
            );
        }
    }

    // ── Pass 2b: border strokes ─────────────────────────────────────────
    let stroke_mesh = |polylines: &[borders::Polyline], width: f32| -> MeshBuilder2d {
        let mut builder = MeshBuilder2d::default();
        for line in polylines {
            let pts: Vec<Vec2> = line.pts.iter().map(|p| react_to_world(*p)).collect();
            builder.add_polyline_strip(&pts, width, line.closed);
        }
        builder
    };
    let segment_mesh = |segments: &[[Point; 2]], width: f32| -> MeshBuilder2d {
        let mut builder = MeshBuilder2d::default();
        for [a, b] in segments {
            builder.add_segment(react_to_world(*a), react_to_world(*b), width);
        }
        builder
    };

    // Province borders: hidden in diplomatic mode, gated to zoomed-in views.
    if *mode != MapMode::Diplomatic {
        let builder = if settings.organic_borders {
            stroke_mesh(&map_borders.province_strokes, 1.5 * REACT_SCALE)
        } else {
            segment_mesh(&map_borders.straight_province_segments, 1.5 * REACT_SCALE)
        };
        if !builder.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                builder.build(),
                Color::srgba(20.0 / 255.0, 15.0 / 255.0, 10.0 / 255.0, 0.5),
                0.7,
                Some(LodGate::PastLabels),
            );
        }
    }

    if settings.organic_borders {
        let country = stroke_mesh(&map_borders.country_strokes, 3.5 * REACT_SCALE);
        if !country.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                country.build(),
                Color::srgba(10.0 / 255.0, 5.0 / 255.0, 0.0, 0.9),
                0.8,
                None,
            );
        }
        let coast = stroke_mesh(&map_borders.coast_strokes, 2.5 * REACT_SCALE);
        if !coast.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                coast.build(),
                Color::srgba(10.0 / 255.0, 5.0 / 255.0, 0.0, 0.85),
                0.9,
                None,
            );
        }
    } else {
        let country = segment_mesh(&map_borders.straight_country_segments, 3.5 * REACT_SCALE);
        if !country.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                country.build(),
                Color::srgba(10.0 / 255.0, 5.0 / 255.0, 0.0, 0.9),
                0.8,
                None,
            );
        }
    }

    // ── Pass 3a: rivers (terrain mode only) ─────────────────────────────
    if *mode == MapMode::Terrain {
        let index: HashMap<(i32, i32), &MapTile> = tiles.iter().map(|t| ((t.q, t.r), t)).collect();
        let mut builder = MeshBuilder2d::default();
        let width = (2.0_f32).max(HEX_SIZE * 0.22);
        for tile in tiles {
            if !tile.has_river || tile.is_sea() {
                continue;
            }
            let p = geometry::hex_to_world(tile.q, tile.r);
            for (nq, nr) in borders::hex_neighbors(tile.q, tile.r) {
                let Some(neighbor) = index.get(&(nq, nr)) else {
                    continue;
                };
                if !neighbor.has_river && !neighbor.is_sea() {
                    continue;
                }
                let np = geometry::hex_to_world(nq, nr);
                let target = if neighbor.is_sea() {
                    p + (np - p) * 0.45
                } else {
                    np
                };
                builder.add_segment(p, target, width);
                // Round-ish cap at each endpoint so meeting segments blend.
                builder.add_circle(p, width / 2.0, 8);
                builder.add_circle(target, width / 2.0, 8);
            }
        }
        if !builder.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                builder.build(),
                Color::srgba(68.0 / 255.0, 140.0 / 255.0, 220.0 / 255.0, 0.95),
                1.0,
                None,
            );
        }
    }

    // ── Pass 3b: railroads (terrain mode, transport toggle) ─────────────
    if *mode == MapMode::Terrain && settings.show_transport_network {
        let mut builder = MeshBuilder2d::default();
        let rw = HEX_SIZE * 0.35;
        let rail_off = 2.0 * REACT_SCALE;
        let tie_half = 3.0 * REACT_SCALE;
        let tie_step = 5.0 * REACT_SCALE;
        let line_w = 1.2 * REACT_SCALE;
        for tile in tiles {
            if tile.is_sea() || !tile.has_railroad {
                continue;
            }
            let p = geometry::hex_to_world(tile.q, tile.r);
            builder.add_segment(
                p + Vec2::new(-rw, rail_off),
                p + Vec2::new(rw, rail_off),
                line_w,
            );
            builder.add_segment(
                p + Vec2::new(-rw, -rail_off),
                p + Vec2::new(rw, -rail_off),
                line_w,
            );
            let mut t = -rw + 2.0 * REACT_SCALE;
            while t <= rw - 2.0 * REACT_SCALE {
                builder.add_segment(
                    p + Vec2::new(t, -tie_half),
                    p + Vec2::new(t, tie_half),
                    line_w,
                );
                t += tie_step;
            }
        }
        if !builder.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                builder.build(),
                Color::srgba(100.0 / 255.0, 60.0 / 255.0, 20.0 / 255.0, 0.8),
                1.1,
                Some(LodGate::Infra),
            );
        }
    }

    // ── Pass 3c: provincial-capital dots (white dot + dark outline) ─────
    {
        let mut dots = MeshBuilder2d::default();
        let mut outlines = MeshBuilder2d::default();
        let dot_r = 2.5 * REACT_SCALE;
        for tile in tiles {
            if !tile.is_capital || tile.is_country_capital || tile.is_sea() {
                continue;
            }
            let p = geometry::hex_to_world(tile.q, tile.r);
            dots.add_circle(p, dot_r, 12);
            outlines.add_ring(p, dot_r, dot_r + 0.8 * REACT_SCALE, 12);
        }
        if !dots.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                dots.build(),
                Color::srgba(1.0, 1.0, 1.0, 0.7),
                1.4,
                None,
            );
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                outlines.build(),
                Color::srgba(0.0, 0.0, 0.0, 0.4),
                1.41,
                None,
            );
        }
    }
}

/// Move-target / fleet-target hex tints, rebuilt whenever the highlight
/// resources change (plumbing for the movement UI).
pub fn rebuild_highlight_layers(
    mut commands: Commands,
    vms: Res<ViewModels>,
    move_targets: Res<MoveTargets>,
    fleet_targets: Res<FleetTargets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    wrap_roots: Query<Entity, With<WrapRoot>>,
    layers: Query<Entity, With<HighlightLayer>>,
    mut built_version: Local<u64>,
) {
    let dirty =
        move_targets.is_changed() || fleet_targets.is_changed() || *built_version != vms.version;
    if !dirty {
        return;
    }
    *built_version = vms.version;
    for entity in &layers {
        commands.entity(entity).despawn();
    }
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    if wrap_roots.is_empty() {
        return;
    }

    let friendly: std::collections::HashSet<u64> = move_targets.friendly.iter().copied().collect();
    let hostile: std::collections::HashSet<u64> = move_targets.hostile.iter().copied().collect();
    let mut groups: Vec<(Color, Vec<Vec2>)> = vec![
        (
            Color::srgba(46.0 / 255.0, 204.0 / 255.0, 64.0 / 255.0, 0.25),
            Vec::new(),
        ),
        (
            Color::srgba(1.0, 65.0 / 255.0, 54.0 / 255.0, 0.25),
            Vec::new(),
        ),
        (
            Color::srgba(64.0 / 255.0, 156.0 / 255.0, 1.0, 0.28),
            Vec::new(),
        ),
    ];
    for tile in tiles {
        if let Some(pid) = tile.province_id {
            if friendly.contains(&pid) {
                groups[0].1.push(geometry::hex_to_world(tile.q, tile.r));
            } else if hostile.contains(&pid) {
                groups[1].1.push(geometry::hex_to_world(tile.q, tile.r));
            }
        }
        if tile.is_sea() && fleet_targets.0.contains(&(tile.q, tile.r)) {
            groups[2].1.push(geometry::hex_to_world(tile.q, tile.r));
        }
    }
    for (color, centers) in groups {
        if centers.is_empty() {
            continue;
        }
        let mesh = meshes.add(geometry::merged_hex_mesh(&centers, HEX_SIZE - 0.5));
        let material = materials.add(color);
        for root in &wrap_roots {
            commands.spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(0.0, 0.0, 1.3),
                HighlightLayer,
                ChildOf(root),
            ));
        }
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
