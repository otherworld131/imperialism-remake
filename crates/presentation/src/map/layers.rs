//! Merged map mesh layers: tile fills for all six map modes, organic
//! fill-correction strips, fog of war, sea zones, the hex grid, border
//! strokes, rivers, and railroads. Everything is merged per color and
//! spawned three times under the wrap roots so the map tiles seamlessly
//! across the horizontal seam.
//!
//! Styling constants mirror `web/src/components/HexMap.tsx`; React sizes are
//! authored against hex 18 and scaled by `REACT_SCALE` here.

use bevy::math::Affine2;
use bevy::prelude::*;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::game::resources::{
    DeployMode, FleetTargets, FreshRail, MoveTargets, RailLinkOptions, RenderSettings, ViewModels,
};
use crate::game::vm::MapTile;
use crate::map::borders::{self, MapBorders};
use crate::map::camera::GameCamera;
use crate::map::geometry::{self, HEX_SIZE};
use crate::map::icons::IconAssets;
use crate::map::lod::LodGate;
use crate::map::organic::{self, Point};
use crate::map::picking::{HoveredHex, SelectedHex};
use crate::map::political::{self, PoliticalRasterCache};
use crate::map::polyline::MeshBuilder2d;
use crate::theme;

/// World units per React canvas unit (hex 24 vs hex 18).
pub const REACT_SCALE: f32 = HEX_SIZE / 18.0;

/// World-space repeat period of the pixel-art ground textures. Matches the
/// texel density of the 64px terrain motif sprites drawn at 1.5 hexes, so
/// ground and motif pixels are the same size on screen.
pub const GROUND_TEX_WORLD: f32 = HEX_SIZE * 1.5;

/// Axial deltas for the rail-link direction indices 0-5 emitted by
/// frontend-api's `MapTile.rail_links`. MUST stay identical to
/// `domain::hex::HEX_DIRECTIONS` (presentation has no domain dependency, so
/// the contract is pinned by `rail_dirs_contract_pinned` below).
/// The renderer relies on the opposite of dir `i` being `(i + 3) % 6` for
/// its draw-each-edge-once dedup.
pub const RAIL_DIRS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// Rail-link quad width in world units. Half a hex keeps the 64x64 track
/// texture's texels square (V maps 32 art texels across this width).
pub const RAIL_TRACK_WIDTH: f32 = HEX_SIZE * 0.5;
/// World-units per repeat of the track texture along a rail quad (64 art
/// texels at the same texel density as `RAIL_TRACK_WIDTH`).
pub const RAIL_TRACK_U_PERIOD: f32 = HEX_SIZE;

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

/// Ghost rail-link preview (card #497): one persistent textured quad shown
/// while a settled engineer is armed and the cursor hovers a neighbouring
/// hex. Rendered like the real track (locked decision #7), slightly
/// translucent; red tint when refused (tooltip explains).
#[derive(Component)]
pub struct RailPreviewGhost;

/// The ghost's endpoint ballast pads — same `rail/Node` texture as built
/// rail, so the preview matches the finished composition (codex F-003).
#[derive(Component)]
pub struct RailPreviewGhostPads;

#[derive(Component)]
pub struct SelectionRing;

/// Pulsing under-glow beneath rail links laid in the last turn resolution
/// (see [`FreshRail`]): freshly built track must be spottable at a glance.
#[derive(Component)]
pub struct FreshRailGlow;

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
    // Rail-link ghost (track + endpoint pads): meshes and textures are
    // filled lazily by `update_rail_preview` (IconAssets may not exist yet
    // at startup). Pads sit just under the track, mirroring the real
    // renderer's node-under-track layering.
    commands.spawn((
        Mesh2d(meshes.add(MeshBuilder2d::default().build())),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 1.34),
        Visibility::Hidden,
        RailPreviewGhostPads,
    ));
    commands.spawn((
        Mesh2d(meshes.add(MeshBuilder2d::default().build())),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 1.35),
        Visibility::Hidden,
        RailPreviewGhost,
    ));
}

pub fn handle_map_mode_input(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<bevy::input_focus::InputFocus>,
    mut mode: ResMut<MapMode>,
) {
    // A focused text input owns the keyboard ('m' typed into the skip-until
    // box must not cycle the map mode).
    if focus.0.is_some() {
        return;
    }
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

/// Ground-texture key for a land tile in terrain mode. Forest tiles without
/// visible Timber are scrub wood (card #540) and use the washed-out
/// `ForestScrub` ground instead of the vivid deep-green `Forest` one.
/// Timber is never a hidden deposit today, but the `resource_hidden` guard
/// keeps a modded hidden-Timber forest from leaking its deposit through the
/// vivid art before a prospector reveals it.
fn ground_texture_name(tile: &MapTile) -> &str {
    let visible_timber = tile.resource.as_deref() == Some("Timber") && !tile.resource_hidden;
    if tile.terrain == "Forest" && !visible_timber {
        "ForestScrub"
    } else {
        &tile.terrain
    }
}

/// Number of authored river-source mountain appearance variants
/// (`terrain/Mountain…River1..N`, card #539).
pub const RIVER_SOURCE_VARIANTS: u32 = 3;

/// Deterministic 1-based appearance variant for a river-source mountain hex.
/// Keyed on the hex coords with an integer hash so maps look varied but the
/// pick is stable across rebuilds and sessions.
#[must_use]
pub fn river_source_variant(q: i32, r: i32) -> u32 {
    let mut h = (q.wrapping_mul(374_761_393) ^ r.wrapping_mul(668_265_263)) as u32;
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    1 + (h ^ (h >> 16)) % RIVER_SOURCE_VARIANTS
}

/// Coords of the hexes where a river originates (card #539): a mountain
/// river tile with at most one river-flagged land neighbor is the head of a
/// generated river path. (Map generation starts every river on a mountain,
/// forbids two rivers from touching, and forbids a path from re-approaching
/// itself, so mid-course mountain tiles always have two river neighbors.
/// Zero land neighbors covers the degenerate single-tile river that flows
/// straight into the sea — it still originates here.)
#[must_use]
pub fn river_source_coords(tiles: &[MapTile]) -> HashSet<(i32, i32)> {
    let river: HashSet<(i32, i32)> = tiles
        .iter()
        .filter(|t| t.has_river && !t.is_sea())
        .map(|t| (t.q, t.r))
        .collect();
    tiles
        .iter()
        .filter(|t| t.terrain == "Mountain" && t.has_river && !t.is_sea())
        .filter(|t| {
            let downstream = borders::hex_neighbors(t.q, t.r)
                .iter()
                .filter(|n| river.contains(*n))
                .count();
            downstream <= 1
        })
        .map(|t| (t.q, t.r))
        .collect()
}

/// Terrain motif sprite name for one land tile in terrain mode
/// (cards #542 / #540 / #539).
///
/// - A tile whose resource is visible to the player uses the integrated
///   `terrain/<Terrain><Resource>` art — the resource is woven into the hex
///   design (grain rows, coal seams, sheep, …) instead of an icon overlay.
///   Hidden deposits keep the plain art until a prospector reveals them;
///   the swap then happens on the next map rebuild (view-model version bump).
/// - The "Show resources" display toggle now switches between plain and
///   resource-integrated tile art (plus the improvement badges).
/// - Forests (#540): Timber present → the vivid `Forest` pines (good,
///   developable timber); no Timber → the sparse, desaturated `ForestScrub`.
///   This is the tile's identity, independent of the display toggle.
/// - Plain grassland stays motif-free (open ground).
/// - Mountains where a river originates (#539) append `River<variant>` to
///   the motif (`MountainRiver2`, `MountainGoldRiver1`, …) — the art shows
///   the stream flowing out of the flank. `river_variant` is the
///   [`river_source_variant`] pick for source hexes, `None` otherwise; like
///   the forest split it is terrain identity, not a display toggle.
pub fn terrain_motif_name(
    tile: &MapTile,
    settings: &RenderSettings,
    river_variant: Option<u32>,
) -> Option<String> {
    if tile.is_sea() {
        return None;
    }
    if tile.terrain == "Forest" {
        // Hidden-deposit guard mirrors the other resource branches (Timber
        // is never hidden today; defensive for mods).
        let timber_visible = tile.resource.as_deref() == Some("Timber")
            && (!tile.resource_hidden || settings.show_hidden_resources);
        return Some(if timber_visible {
            "Forest".to_string()
        } else {
            "ForestScrub".to_string()
        });
    }
    let base = match tile.resource.as_deref() {
        Some(resource)
            if settings.show_resources
                && (!tile.resource_hidden || settings.show_hidden_resources) =>
        {
            Some(format!("{}{}", tile.terrain, resource))
        }
        _ if tile.terrain == "Grassland" => None,
        _ => Some(tile.terrain.clone()),
    };
    match (base, river_variant) {
        (Some(base), Some(v)) if tile.terrain == "Mountain" => Some(format!("{base}River{v}")),
        (base, _) => base,
    }
}

/// Fill spec for one tile: a multiplier color plus an optional repeating
/// pixel-art ground texture (keyed by terrain name for mesh grouping).
/// Terrain mode samples ground textures for land, the sea is textured in
/// every mode, and the remaining modes keep their flat data-driven fills.
pub fn tile_fill_spec(
    tile: &MapTile,
    mode: MapMode,
    fill_map: &HashMap<String, Color>,
    icons: &IconAssets,
) -> (Color, Option<(String, Handle<Image>)>) {
    let ground = if tile.is_sea() {
        icons.get("ground", "Sea").map(|h| ("Sea".to_string(), h))
    } else if mode == MapMode::Terrain {
        let key = ground_texture_name(tile);
        icons
            .get("ground", key)
            .map(|h| (key.to_string(), h))
            .or_else(|| {
                // Scrub texture missing → the vivid forest ground still
                // beats a flat fill.
                icons
                    .get("ground", &tile.terrain)
                    .map(|h| (tile.terrain.clone(), h))
            })
    } else {
        None
    };
    match ground {
        Some(texture) => {
            let color = if tile.is_sea() || tile.owner_color.is_empty() {
                Color::WHITE
            } else {
                // Ownership tint: multiply the texture by white nudged
                // toward the nation color, mirroring the flat-fill amounts.
                let amount = if tile.is_incorporated_minor {
                    0.10
                } else {
                    0.15
                };
                theme::terrain_nation_tint(
                    Color::WHITE,
                    theme::nation_color(&tile.owner_color),
                    amount,
                )
            };
            (color, Some(texture))
        }
        None => (tile_fill_color(tile, mode, fill_map), None),
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
    fresh_rail: Res<FreshRail>,
    mut cache: ResMut<BordersCache>,
    mut raster_cache: ResMut<PoliticalRasterCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    icons: Res<IconAssets>,
    wrap_roots: Query<(Entity, &Transform), With<WrapRoot>>,
    layers: Query<Entity, With<StaticLayer>>,
    mut built: Local<Option<StaticKey>>,
    mut rebuild_count: Local<u32>,
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
    // Rebuilds are rare (data version bump / mode switch / toggle) — the
    // counter in this log doubles as the per-frame-rebuild canary.
    let build_started = std::time::Instant::now();
    *rebuild_count += 1;

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

    let roots: Vec<(Entity, f32)> = if wrap_roots.is_empty() {
        [-1.0f32, 0.0, 1.0]
            .into_iter()
            .map(|offset| {
                let x = offset * bounds.width_px;
                let entity = commands
                    .spawn((
                        WrapRoot,
                        Transform::from_xyz(x, 0.0, 0.0),
                        Visibility::default(),
                    ))
                    .id();
                (entity, x)
            })
            .collect()
    } else {
        // Re-anchor existing roots to the current map width: a new game or
        // load can change `width_px`, and both the wrap-copy placement and
        // the textured UV phase shift below must follow it. Sort by the old
        // x so each root keeps its -1/0/+1 slot.
        let mut existing: Vec<(Entity, f32)> = wrap_roots
            .iter()
            .map(|(entity, transform)| (entity, transform.translation.x))
            .collect();
        existing.sort_by(|a, b| a.1.total_cmp(&b.1));
        existing
            .into_iter()
            .zip([-1.0f32, 0.0, 1.0])
            .map(|((entity, _), offset)| {
                let x = offset * bounds.width_px;
                commands
                    .entity(entity)
                    .insert(Transform::from_xyz(x, 0.0, 0.0));
                (entity, x)
            })
            .collect()
    };

    let mut classify_elapsed = None;
    if cache.version != vms.version || cache.data.is_none() {
        let classify_started = std::time::Instant::now();
        cache.data = Some(borders::classify(tiles, f64::from(HEX_SIZE)));
        cache.version = vms.version;
        classify_elapsed = Some(classify_started.elapsed());
    }
    let Some(map_borders) = cache.data.as_ref() else {
        return;
    };

    // Shared spawner: one mesh+material, three wrap copies. A ground
    // texture switches the mesh to world-aligned UVs so the repeating
    // pixel pattern flows continuously across hexes and strips.
    let spawn_mesh = |commands: &mut Commands,
                      meshes: &mut Assets<Mesh>,
                      materials: &mut Assets<ColorMaterial>,
                      mut mesh: Mesh,
                      color: Color,
                      texture: Option<Handle<Image>>,
                      z: f32,
                      gate: Option<LodGate>| {
        if texture.is_some() {
            geometry::apply_world_uvs(&mut mesh, GROUND_TEX_WORLD);
        }
        let mesh = meshes.add(mesh);
        // Flat fills share one material across the three wrap copies.
        // Textured fills need one material per copy: the copies translate
        // a shared mesh whose UVs were baked from untranslated positions,
        // so each copy shifts the texture phase by its world offset to
        // keep the pattern continuous across the wrap seam.
        let flat_material = if texture.is_none() {
            Some(materials.add(color))
        } else {
            None
        };
        for &(root, root_x) in &roots {
            let material = match &flat_material {
                Some(handle) => handle.clone(),
                None => materials.add(ColorMaterial {
                    color,
                    texture: texture.clone(),
                    uv_transform: Affine2::from_translation(Vec2::new(
                        root_x / GROUND_TEX_WORLD,
                        0.0,
                    )),
                    ..default()
                }),
            };
            let mut entity = commands.spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material),
                Transform::from_xyz(0.0, 0.0, z),
                StaticLayer,
                ChildOf(root),
            ));
            if let Some(gate) = gate {
                entity.insert(gate);
            }
        }
    };

    // Variant spawner for meshes carrying their own (mesh-local) UVs — the
    // rail track/node textures. Unlike `spawn_mesh` it must NOT rewrite the
    // UVs to world space nor phase-shift per wrap copy: the UVs are baked
    // along each segment, identical in every copy, so one shared material
    // suffices.
    let spawn_mesh_local_uv = |commands: &mut Commands,
                               meshes: &mut Assets<Mesh>,
                               materials: &mut Assets<ColorMaterial>,
                               mesh: Mesh,
                               color: Color,
                               texture: Handle<Image>,
                               z: f32,
                               gate: Option<LodGate>| {
        let mesh = meshes.add(mesh);
        let material = materials.add(ColorMaterial {
            color,
            texture: Some(texture),
            ..default()
        });
        for &(root, _) in &roots {
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
    // Political + overlay modes render land as one chunky pixel raster
    // (card #531) instead of vector fills with organic borders; terrain
    // mode keeps the full organic pipeline.
    let pixel_political = *mode != MapMode::Terrain;

    // ── Pass 1: tile fills, grouped per (ground texture, color) ─────────
    // In the pixel-political modes every tile (land included) lays down the
    // pixel-art water underlay: the land raster above is texel-quantized, so
    // its staircase coastline must meet water pixels, not background.
    let sea_spec = match icons.get("ground", "Sea") {
        Some(handle) => (Color::WHITE, Some(("Sea".to_string(), handle))),
        None => (theme::terrain_color("Sea"), None),
    };
    type FillGroup = (Color, Option<Handle<Image>>, Vec<Vec2>);
    let mut fill_groups: BTreeMap<(String, u32), FillGroup> = BTreeMap::new();
    for tile in tiles {
        let (color, texture) = if pixel_political {
            sea_spec.clone()
        } else {
            tile_fill_spec(tile, *mode, &fill_map, &icons)
        };
        let (tex_key, handle) = match texture {
            Some((key, handle)) => (key, Some(handle)),
            None => (String::new(), None),
        };
        fill_groups
            .entry((tex_key, color_key(color)))
            .or_insert_with(|| (color, handle, Vec::new()))
            .2
            .push(geometry::hex_to_world(tile.q, tile.r));
    }
    // Slight overdraw hides hairline seams between adjacent hexes (with
    // world-aligned textures the overlap samples identical pixels anyway).
    let radius = HEX_SIZE + 0.45;
    for (_, (color, texture, centers)) in fill_groups {
        spawn_mesh(
            &mut commands,
            &mut meshes,
            &mut materials,
            geometry::merged_hex_mesh(&centers, radius),
            color,
            texture,
            0.0,
            None,
        );
    }

    // ── Pass 1p: pixel-political land raster (card #531) ────────────────
    // One nearest-sampled texture bakes the nation fills, country borders
    // and coastlines as chunky pixel staircases (see `map/political.rs`).
    // Cached per (version, mode): display-toggle rebuilds reuse the pixels.
    let mut raster_elapsed = None;
    if pixel_political {
        if raster_cache.key != Some((vms.version, *mode)) {
            let raster_started = std::time::Instant::now();
            if let Some(raster) = political::build_raster(tiles, *mode, &fill_map) {
                raster_cache.handle = images.add(raster.image);
                raster_cache.min = raster.min;
                raster_cache.max = raster.max;
                raster_cache.key = Some((vms.version, *mode));
            }
            raster_elapsed = Some(raster_started.elapsed());
        }
        if raster_cache.key.is_some() {
            spawn_mesh_local_uv(
                &mut commands,
                &mut meshes,
                &mut materials,
                political::raster_quad_mesh(raster_cache.min, raster_cache.max),
                Color::WHITE,
                raster_cache.handle.clone(),
                0.15,
                None,
            );
        }
    }

    // ── Pass 1a: terrain motif sprites (terrain mode) ───────────────────
    // Every land tile gets an authored terrain icon (mountains, forest,
    // swamp, …) layered over its color fill, so terrain reads as art and
    // not just a flat tint. Tiles with a visible resource swap to the
    // integrated `<Terrain><Resource>` variant (cards #542/#540) — the
    // resource is part of the hex art, not an overlay. These live in the
    // static layer: a prospector's find bumps the view-model version and
    // rebuilds them with the revealed variant.
    if *mode == MapMode::Terrain {
        let terrain_size = HEX_SIZE * 1.5;
        // Mountains where a river originates get the outflow-stream art
        // (card #539), in one of a few hash-picked appearance variants.
        let river_sources = river_source_coords(tiles);
        for tile in tiles {
            // Capital hexes show their city art instead (card #541) — a
            // motif underneath would just clutter the palace / town cluster.
            if tile.is_capital {
                continue;
            }
            let river_variant = river_sources
                .contains(&(tile.q, tile.r))
                .then(|| river_source_variant(tile.q, tile.r));
            // Plain grassland reads cleaner as open ground: no motif (the
            // grass-tuft art looked like stray clutter on every tile).
            let Some(motif) = terrain_motif_name(tile, &settings, river_variant) else {
                continue;
            };
            // Missing variant art falls back to the plain terrain motif.
            let Some(image) = icons
                .get("terrain", &motif)
                .or_else(|| icons.get("terrain", &tile.terrain))
            else {
                continue;
            };
            let p = geometry::hex_to_world(tile.q, tile.r);
            for &(root, _) in &roots {
                commands.spawn((
                    Sprite {
                        image: image.clone(),
                        custom_size: Some(Vec2::splat(terrain_size)),
                        color: Color::WHITE.with_alpha(0.92),
                        ..default()
                    },
                    Transform::from_xyz(p.x, p.y, 0.12),
                    StaticLayer,
                    ChildOf(root),
                ));
            }
        }
    }

    // ── Pass 1b: organic fill-correction strips ─────────────────────────
    // For every coast / country edge, the smoothed curve wanders around the
    // straight hex edge. Cover each excursion with the color of the side it
    // pokes into: tile color past the baseline outward, sea/neighbor color
    // inward. Result: fills meet exactly on the smoothed border like the
    // web's clipped-canvas rendering. Terrain mode only — the
    // pixel-political raster owns its own (texel-stepped) coastline.
    if settings.organic_borders && !pixel_political {
        type StripGroup = (Color, Option<Handle<Image>>, MeshBuilder2d);
        let mut strip_builders: BTreeMap<(String, u32), StripGroup> = BTreeMap::new();
        // Off-map water (no neighbor tile) matches the sea fill (the outer
        // `sea_spec`): textured pixel water, flat color as fallback.
        let mut add_strips = |strips: &[borders::FillStrip]| {
            for strip in strips {
                let tile_spec = tile_fill_spec(&tiles[strip.tile_idx], *mode, &fill_map, &icons);
                let other_spec = match strip.neighbor_idx {
                    Some(ni) => tile_fill_spec(&tiles[ni], *mode, &fill_map, &icons),
                    None => sea_spec.clone(),
                };
                if tile_spec == other_spec {
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
                for ((color, texture), out) in [(tile_spec, &out_rail), (other_spec, &in_rail)] {
                    let (tex_key, handle) = match texture {
                        Some((key, handle)) => (key, Some(handle)),
                        None => (String::new(), None),
                    };
                    strip_builders
                        .entry((tex_key, color_key(color)))
                        .or_insert_with(|| (color, handle, MeshBuilder2d::default()))
                        .2
                        .add_ribbon(&base_rail, out);
                }
            }
        };
        add_strips(&map_borders.coast_strips);
        add_strips(&map_borders.country_strips);
        for (_, (color, texture, builder)) in strip_builders {
            if !builder.is_empty() {
                spawn_mesh(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    builder.build(),
                    color,
                    texture,
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
                None,
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
            None,
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
                None,
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
                None,
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
    // The pixel-political modes always use the straight hex-edge segments —
    // organic curves would clash with the texel-stepped raster below.
    if *mode != MapMode::Diplomatic {
        let builder = if settings.organic_borders && !pixel_political {
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
                None,
                0.7,
                Some(LodGate::PastLabels),
            );
        }
    }

    // Country + coast strokes: the pixel-political raster bakes both as
    // texel staircases, so only terrain mode draws them as vector strokes.
    if pixel_political {
        // Nothing — borders live in the raster (Pass 1p).
    } else if settings.organic_borders {
        let country = stroke_mesh(&map_borders.country_strokes, 3.5 * REACT_SCALE);
        if !country.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                country.build(),
                Color::srgba(10.0 / 255.0, 5.0 / 255.0, 0.0, 0.9),
                None,
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
                None,
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
                None,
                0.8,
                None,
            );
        }
    }

    // ── Pass 3a: rivers (terrain mode only) ─────────────────────────────
    // Card #539: river courses meander. Each hex-center-to-hex-center
    // segment runs through the organic displacement pipeline with a
    // dedicated seeded noise field (stable per rebuild), endpoints pinned at
    // the hex centers so chained segments join continuously and the
    // amplitude tuned to stay inside the two hexes' corridor.
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
                let to_sea = neighbor.is_sea();
                if !neighbor.has_river && !to_sea {
                    continue;
                }
                // A land-land river edge is visited from both endpoints;
                // draw it once, from the lexicographically smaller coord.
                // (Reversing a displaced segment mirrors its meander, so the
                // duplicate would double-image the curve. Sea mouths are
                // only ever visited from the land side.)
                if !to_sea && (nq, nr) < (tile.q, tile.r) {
                    continue;
                }
                let np = geometry::hex_to_world(nq, nr);
                let target = if to_sea { p + (np - p) * 0.45 } else { np };
                if settings.organic_borders {
                    let curve = organic::river_polyline(
                        [f64::from(p.x), f64::from(p.y)],
                        [f64::from(target.x), f64::from(target.y)],
                        f64::from(HEX_SIZE),
                    );
                    let pts: Vec<Vec2> = curve
                        .iter()
                        .map(|c| Vec2::new(c[0] as f32, c[1] as f32))
                        .collect();
                    builder.add_polyline_strip(&pts, width, false);
                } else {
                    builder.add_segment(p, target, width);
                }
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
                None,
                1.0,
                None,
            );
        }
    }

    // ── Pass 3b: rail links (terrain mode, transport toggle) ────────────
    // Each physical link is a textured quad from hex center to hex center
    // (hand-drawn seamless track texture, arc-length UVs), with a ballast
    // node pad under every railhead hex to hide the butt joints where quads
    // meet. Edges appear on both endpoints as opposite direction indices;
    // drawing only dirs {0,1,2} (the opposite is (i+3)%6 ∈ {3,4,5}) gives a
    // free dedup so each link is drawn exactly once.
    if *mode == MapMode::Terrain && settings.show_transport_network {
        // Quad width HEX_SIZE/2 with one texture repeat per HEX_SIZE keeps
        // the 64x64 art's texels square (32 texels across the width).
        let track_w = RAIL_TRACK_WIDTH;
        let u_period = RAIL_TRACK_U_PERIOD;
        let track_tex = icons.get("rail", "Track");
        let node_tex = icons.get("rail", "Node");

        let mut track = MeshBuilder2d::default();
        let mut nodes = MeshBuilder2d::default();
        let mut fallback = MeshBuilder2d::default();
        let mut glow = MeshBuilder2d::default();
        for tile in tiles {
            if tile.is_sea() {
                continue;
            }
            let a = geometry::hex_to_world(tile.q, tile.r);
            if node_tex.is_some() && !tile.rail_links.is_empty() {
                nodes.add_textured_quad(a, HEX_SIZE * 0.5);
            }
            for &dir in &tile.rail_links {
                if dir > 2 {
                    continue;
                }
                let (dq, dr) = RAIL_DIRS[dir as usize];
                let b = geometry::hex_to_world(tile.q + dq, tile.r + dr);
                if track_tex.is_some() {
                    track.add_textured_segment(a, b, track_w, u_period);
                } else {
                    fallback.add_segment(a, b, HEX_SIZE * 0.12);
                }
                // Track laid in the last turn resolution gets a pulsing
                // golden under-glow for one turn (see `FreshRail`).
                if fresh_rail
                    .fresh_edges
                    .contains(&((tile.q, tile.r), (tile.q + dq, tile.r + dr)))
                {
                    glow.add_segment(a, b, track_w * 1.9);
                    glow.add_circle(a, track_w * 0.95, 16);
                    glow.add_circle(b, track_w * 0.95, 16);
                }
            }
        }
        if !glow.is_empty() {
            // Under the ballast pads and track so the pixel art stays crisp;
            // `animate_fresh_rail` pulses the shared material's alpha.
            let mesh = meshes.add(glow.build());
            let material = materials.add(Color::srgba(1.0, 0.84, 0.25, 0.65));
            for &(root, _) in &roots {
                commands.spawn((
                    Mesh2d(mesh.clone()),
                    MeshMaterial2d(material.clone()),
                    Transform::from_xyz(0.0, 0.0, 1.04),
                    StaticLayer,
                    FreshRailGlow,
                    LodGate::Infra,
                    ChildOf(root),
                ));
            }
        }
        if let (Some(tex), false) = (node_tex, nodes.is_empty()) {
            spawn_mesh_local_uv(
                &mut commands,
                &mut meshes,
                &mut materials,
                nodes.build(),
                Color::WHITE,
                tex,
                1.05,
                Some(LodGate::Infra),
            );
        }
        if let (Some(tex), false) = (track_tex, track.is_empty()) {
            spawn_mesh_local_uv(
                &mut commands,
                &mut meshes,
                &mut materials,
                track.build(),
                Color::WHITE,
                tex,
                1.1,
                Some(LodGate::Infra),
            );
        }
        if !fallback.is_empty() {
            spawn_mesh(
                &mut commands,
                &mut meshes,
                &mut materials,
                fallback.build(),
                Color::srgba(100.0 / 255.0, 60.0 / 255.0, 20.0 / 255.0, 0.8),
                None,
                1.1,
                Some(LodGate::Infra),
            );
        }
    }

    // Provincial capitals no longer draw the white marker dot — the
    // province-town city art in the marker layer is the tile's identity now
    // (card #541).

    let classify_note = match classify_elapsed {
        Some(classify) => format!("borders classify {classify:.1?}"),
        None => "borders cached".to_string(),
    };
    let raster_note = match raster_elapsed {
        Some(raster) => format!(", political raster {raster:.1?}"),
        None if pixel_political => ", political raster cached".to_string(),
        None => String::new(),
    };
    info!(
        "map layers rebuild #{} (version {}, {} tiles): {}{}, total {:.1?}",
        *rebuild_count,
        vms.version,
        tiles.len(),
        classify_note,
        raster_note,
        build_started.elapsed(),
    );
}

/// Move-target / fleet-target / deploy-target hex tints (plus the
/// prospector's red ✗ on already-searched tiles), rebuilt whenever the
/// highlight resources change.
pub fn rebuild_highlight_layers(
    mut commands: Commands,
    vms: Res<ViewModels>,
    move_targets: Res<MoveTargets>,
    fleet_targets: Res<FleetTargets>,
    deploy: Res<DeployMode>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    wrap_roots: Query<Entity, With<WrapRoot>>,
    layers: Query<Entity, With<HighlightLayer>>,
    mut built_version: Local<u64>,
) {
    let dirty = move_targets.is_changed()
        || fleet_targets.is_changed()
        || deploy.is_changed()
        || *built_version != vms.version;
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
    let empty = std::collections::HashSet::new();
    let (deployable, prospected) = match deploy.0.as_ref() {
        Some(state) => (&state.deployable, &state.prospected),
        None => (&empty, &empty),
    };
    let mut groups: Vec<(Color, Vec<Vec2>)> = vec![
        (
            Color::srgba(46.0 / 255.0, 204.0 / 255.0, 64.0 / 255.0, 0.25),
            Vec::new(),
        ),
        (
            Color::srgba(1.0, 65.0 / 255.0, 54.0 / 255.0, 0.25),
            Vec::new(),
        ),
        // Fleet move targets: bright cyan at a higher alpha than the army
        // tints — the old dark-blue 0.28 wash was nearly invisible on the
        // sea texture (a zone-perimeter outline is added below too).
        (
            Color::srgba(80.0 / 255.0, 200.0 / 255.0, 1.0, 0.42),
            Vec::new(),
        ),
        // Deployable tiles (civilian deploy mode), green like the web.
        (
            Color::srgba(46.0 / 255.0, 204.0 / 255.0, 64.0 / 255.0, 0.30),
            Vec::new(),
        ),
    ];
    let mut cross_builder = MeshBuilder2d::default();
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
        if deployable.contains(&(tile.q, tile.r)) {
            groups[3].1.push(geometry::hex_to_world(tile.q, tile.r));
        }
        if prospected.contains(&(tile.q, tile.r)) {
            // Red ✗: this tile was already prospected.
            let p = geometry::hex_to_world(tile.q, tile.r);
            let arm = HEX_SIZE * 0.32;
            let width = 1.6 * REACT_SCALE;
            cross_builder.add_segment(p + Vec2::new(-arm, -arm), p + Vec2::new(arm, arm), width);
            cross_builder.add_segment(p + Vec2::new(-arm, arm), p + Vec2::new(arm, -arm), width);
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
    // Bright outline around each reachable sea zone: only edges whose
    // neighbour is not itself a fleet target, so the whole zone reads as one
    // clearly bounded region on any sea shade.
    if !fleet_targets.0.is_empty()
        && let Some(map_width) = tiles.first().map(|t| t.map_width)
    {
        let verts = borders::hex_vertices(f64::from(HEX_SIZE));
        let mut outline = MeshBuilder2d::default();
        for &(q, r) in &fleet_targets.0 {
            let [px, py] = borders::hex_to_pixel(q, r, f64::from(HEX_SIZE));
            for (d, (nq, nr)) in borders::hex_neighbors(q, r).iter().enumerate() {
                if fleet_targets
                    .0
                    .contains(&borders::wrap_axial(*nq, *nr, map_width))
                {
                    continue;
                }
                let v1 = verts[d];
                let v2 = verts[(d + 1) % 6];
                outline.add_segment(
                    react_to_world([px + v1[0], py + v1[1]]),
                    react_to_world([px + v2[0], py + v2[1]]),
                    2.4 * REACT_SCALE,
                );
            }
        }
        if !outline.is_empty() {
            let mesh = meshes.add(outline.build());
            let material = materials.add(Color::srgba(0.75, 0.97, 1.0, 0.95));
            for root in &wrap_roots {
                commands.spawn((
                    Mesh2d(mesh.clone()),
                    MeshMaterial2d(material.clone()),
                    Transform::from_xyz(0.0, 0.0, 1.32),
                    HighlightLayer,
                    ChildOf(root),
                ));
            }
        }
    }
    if !cross_builder.is_empty() {
        let mesh = meshes.add(cross_builder.build());
        let material = materials.add(Color::srgba(0.95, 0.25, 0.2, 0.85));
        for root in &wrap_roots {
            commands.spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(0.0, 0.0, 1.35),
                HighlightLayer,
                ChildOf(root),
            ));
        }
    }
}

/// Pulse the fresh-rail under-glow (card: newly laid track has very quiet
/// feedback). The wrap copies clone the same material handle (same asset),
/// so the per-copy writes are idempotent re-writes of one shared asset; the
/// alpha floor keeps the highlight obvious even in still screenshots.
pub fn animate_fresh_rail(
    time: Res<Time>,
    glows: Query<&MeshMaterial2d<ColorMaterial>, With<FreshRailGlow>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let wave = (time.elapsed_secs() * 3.4).sin() * 0.5 + 0.5;
    let alpha = 0.45 + wave * 0.4;
    for material in &glows {
        if let Some(mat) = materials.get_mut(&material.0) {
            mat.color.set_alpha(alpha);
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

/// Show/refresh the rail-link ghost (card #497): visible only while an armed
/// settled engineer's `RailLinkOptions` are live and the hovered hex is one
/// of its six neighbours. The mesh is regenerated only when the
/// (origin, target, verdict) triple changes; position follows the ring
/// wrap-normalization.
pub fn update_rail_preview(
    hovered: Res<HoveredHex>,
    rail: Res<RailLinkOptions>,
    icons: Option<Res<IconAssets>>,
    bounds: Option<Res<MapBounds>>,
    camera: Query<
        &Transform,
        (
            With<GameCamera>,
            Without<RailPreviewGhost>,
            Without<RailPreviewGhostPads>,
        ),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut ghosts: ParamSet<(
        Query<
            (
                &Mesh2d,
                &MeshMaterial2d<ColorMaterial>,
                &mut Transform,
                &mut Visibility,
            ),
            With<RailPreviewGhost>,
        >,
        Query<
            (
                &Mesh2d,
                &MeshMaterial2d<ColorMaterial>,
                &mut Transform,
                &mut Visibility,
            ),
            With<RailPreviewGhostPads>,
        >,
    )>,
    mut cache: Local<Option<((i32, i32), (i32, i32), bool)>>,
) {
    let desired = rail.0.as_ref().and_then(|state| {
        let hov = hovered.0?;
        let opt = state.options.iter().find(|o| (o.q, o.r) == hov)?;
        Some((state.origin, hov, opt.allowed && opt.affordable))
    });
    let Some((origin, target, ok)) = desired else {
        *cache = None;
        if let Ok((_, _, _, mut visibility)) = ghosts.p0().single_mut() {
            *visibility = Visibility::Hidden;
        }
        if let Ok((_, _, _, mut visibility)) = ghosts.p1().single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    // Allowed: the ghost looks like the real track, only slightly translucent
    // so it reads as "not built yet". Refused: red tint. Shared by track and
    // pads so the whole composite tints together.
    let tint = if ok {
        Color::srgba(1.0, 1.0, 1.0, 0.8)
    } else {
        Color::srgba(1.0, 0.4, 0.35, 0.65)
    };
    let rebuild = *cache != Some((origin, target, ok));
    if rebuild {
        *cache = Some((origin, target, ok));
    }
    let a = geometry::hex_to_world(origin.0, origin.1);
    let b_rel = geometry::hex_to_world(target.0, target.1) - a;
    let camera_x = camera.single().map(|t| t.translation.x).unwrap_or(0.0);
    let mut pos = a;
    if let Some(bounds) = bounds.as_deref() {
        pos.x += ((camera_x - pos.x) / bounds.width_px).round() * bounds.width_px;
    }

    // Track quad plus the two endpoint ballast pads, mirroring the real
    // renderer's composition. The closure is applied to both ghost entities
    // (their ParamSet queries have distinct types, so no loop over them).
    let mut apply = |mesh2d: &Mesh2d,
                     material2d: &MeshMaterial2d<ColorMaterial>,
                     transform: &mut Transform,
                     visibility: &mut Visibility,
                     is_track: bool| {
        if rebuild {
            let mut builder = MeshBuilder2d::default();
            if is_track {
                builder.add_textured_segment(
                    Vec2::ZERO,
                    b_rel,
                    RAIL_TRACK_WIDTH,
                    RAIL_TRACK_U_PERIOD,
                );
            } else {
                builder.add_textured_quad(Vec2::ZERO, HEX_SIZE * 0.5);
                builder.add_textured_quad(b_rel, HEX_SIZE * 0.5);
            }
            let _ = meshes.insert(mesh2d.0.id(), builder.build());
        }
        if let Some(mat) = materials.get_mut(material2d.0.id()) {
            mat.color = tint;
            if mat.texture.is_none() {
                let tex_name = if is_track { "Track" } else { "Node" };
                mat.texture = icons.as_deref().and_then(|i| i.get("rail", tex_name));
            }
        }
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        *visibility = Visibility::Visible;
    };
    if let Ok((mesh2d, material2d, mut transform, mut visibility)) = ghosts.p0().single_mut() {
        apply(mesh2d, material2d, &mut transform, &mut visibility, true);
    }
    if let Ok((mesh2d, material2d, mut transform, mut visibility)) = ghosts.p1().single_mut() {
        apply(mesh2d, material2d, &mut transform, &mut visibility, false);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(terrain: &str, owner_color: &str) -> MapTile {
        MapTile {
            q: 0,
            r: 0,
            map_width: 10,
            map_height: 10,
            terrain: terrain.to_string(),
            owner: String::new(),
            owner_color: owner_color.to_string(),
            nation_id: 0,
            province: String::new(),
            province_id: None,
            is_capital: false,
            is_country_capital: false,
            is_minor: false,
            is_incorporated_minor: false,
            incorporated_nation_id: None,
            is_anarchic: false,
            is_prospected: false,
            resource: None,
            resource_hidden: false,
            improvement_level: 0,
            max_improvement_level: 0,
            rail_links: Vec::new(),
            has_depot: false,
            has_port: false,
            has_fort: false,
            has_river: false,
            fort_level: 0,
            port_blockaded: false,
            army_unit_count: 0,
            army_firepower: 0.0,
            army_composition: None,
            naval_ship_count: 0,
            naval_firepower: 0,
            civilian_on_tile: None,
            visible: true,
            visual_group: None,
        }
    }

    /// Pins the rail direction contract literals (review F-010: presentation
    /// cannot import `domain::hex::HEX_DIRECTIONS`, so this guards against
    /// accidental local edits, not cross-crate drift — the doc on `RAIL_DIRS`
    /// records the source of truth). Also asserts opposite(i) == (i+3) % 6.
    #[test]
    fn rail_dirs_contract_pinned() {
        assert_eq!(
            RAIL_DIRS,
            [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)]
        );
        for i in 0..6 {
            let (dq, dr) = RAIL_DIRS[i];
            let (oq, or) = RAIL_DIRS[(i + 3) % 6];
            assert_eq!((dq + oq, dr + or), (0, 0), "dir {i} opposite mismatch");
        }
    }

    fn icons_with_ground() -> IconAssets {
        IconAssets::for_test(&crate::map::icons::GROUND_TEXTURES.map(|name| ("ground", name)))
    }

    #[test]
    fn terrain_mode_land_uses_ground_texture_with_white_tint() {
        let (color, texture) = tile_fill_spec(
            &tile("Grassland", ""),
            MapMode::Terrain,
            &HashMap::new(),
            &icons_with_ground(),
        );
        let (key, _) = texture.expect("grassland ground texture");
        assert_eq!(key, "Grassland");
        assert_eq!(color, Color::WHITE);
    }

    #[test]
    fn terrain_mode_owned_land_tints_the_texture() {
        let (color, texture) = tile_fill_spec(
            &tile("Hills", "Red"),
            MapMode::Terrain,
            &HashMap::new(),
            &icons_with_ground(),
        );
        assert_eq!(texture.expect("hills ground texture").0, "Hills");
        assert_ne!(color, Color::WHITE, "nation tint must survive texturing");
    }

    #[test]
    fn sea_is_textured_in_every_mode() {
        for mode in MapMode::ALL {
            let (color, texture) = tile_fill_spec(
                &tile("Sea", ""),
                mode,
                &HashMap::new(),
                &icons_with_ground(),
            );
            assert_eq!(texture.expect("sea texture").0, "Sea", "mode {mode:?}");
            assert_eq!(color, Color::WHITE, "mode {mode:?}");
        }
    }

    #[test]
    fn political_mode_land_stays_flat() {
        let (color, texture) = tile_fill_spec(
            &tile("Grassland", ""),
            MapMode::Political,
            &HashMap::new(),
            &icons_with_ground(),
        );
        assert!(texture.is_none(), "political land must not be textured");
        assert_eq!(color, theme::terrain_color("Grassland"));
    }

    /// Regression (review F-005/F-008): wrap roots are created once and
    /// reused, so a rebuild for a map of a different width must re-anchor
    /// them — and the textured materials' UV phase shifts must follow,
    /// since both derive from the roots' world x.
    #[test]
    fn wrap_roots_follow_map_width_changes() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<ColorMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<ViewModels>();
        app.init_resource::<MapMode>();
        app.init_resource::<RenderSettings>();
        app.init_resource::<FreshRail>();
        app.init_resource::<BordersCache>();
        app.init_resource::<PoliticalRasterCache>();
        app.insert_resource(icons_with_ground());
        app.add_systems(Update, rebuild_layers);

        let set_map = |app: &mut App, map_width: i32, version: u64| {
            let mut tiles = vec![tile("Grassland", ""), tile("Sea", "")];
            tiles[1].q = 1;
            for t in &mut tiles {
                t.map_width = map_width;
            }
            let mut vms = app.world_mut().resource_mut::<ViewModels>();
            vms.map = Some(tiles);
            vms.version = version;
        };
        let root_xs = |app: &mut App| -> Vec<f32> {
            let mut xs: Vec<f32> = app
                .world_mut()
                .query_filtered::<&Transform, With<WrapRoot>>()
                .iter(app.world())
                .map(|t| t.translation.x)
                .collect();
            xs.sort_by(f32::total_cmp);
            xs
        };
        // Distinct UV phase offsets of the textured materials referenced by
        // the *live* static-layer entities (stale assets from despawned
        // layers may linger in Assets, so go through the entities).
        let texture_phases = |app: &mut App| -> Vec<f32> {
            let handles: Vec<Handle<ColorMaterial>> = app
                .world_mut()
                .query_filtered::<&MeshMaterial2d<ColorMaterial>, With<StaticLayer>>()
                .iter(app.world())
                .map(|m| m.0.clone())
                .collect();
            let materials = app.world().resource::<Assets<ColorMaterial>>();
            let mut phases: Vec<f32> = handles
                .iter()
                .filter_map(|h| materials.get(h))
                .filter(|m| m.texture.is_some())
                .map(|m| m.uv_transform.translation.x)
                .collect();
            phases.sort_by(f32::total_cmp);
            phases.dedup();
            phases
        };
        let expected_phases =
            |width: f32| vec![-width / GROUND_TEX_WORLD, 0.0, width / GROUND_TEX_WORLD];

        set_map(&mut app, 10, 1);
        app.update();
        let width_a = geometry::world_width_px(10);
        assert_eq!(root_xs(&mut app), vec![-width_a, 0.0, width_a]);
        assert_eq!(texture_phases(&mut app), expected_phases(width_a));

        set_map(&mut app, 20, 2);
        app.update();
        let width_b = geometry::world_width_px(20);
        assert_eq!(
            root_xs(&mut app),
            vec![-width_b, 0.0, width_b],
            "roots must re-anchor to the new map width"
        );
        assert_eq!(
            texture_phases(&mut app),
            expected_phases(width_b),
            "textured UV phase shifts must follow the new map width"
        );
    }

    fn tile_with_resource(terrain: &str, resource: &str, hidden: bool) -> MapTile {
        let mut t = tile(terrain, "");
        t.resource = Some(resource.to_string());
        t.resource_hidden = hidden;
        t
    }

    /// Cards #542/#540: visible resources swap the terrain motif to the
    /// integrated `<Terrain><Resource>` variant; hidden deposits keep the
    /// plain art until revealed; forests key vivid-vs-scrub on Timber.
    #[test]
    fn terrain_motif_integrates_visible_resources() {
        let settings = RenderSettings::default();
        assert_eq!(
            terrain_motif_name(&tile_with_resource("Hills", "Coal", false), &settings, None),
            Some("HillsCoal".to_string())
        );
        assert_eq!(
            terrain_motif_name(
                &tile_with_resource("Grassland", "Grain", false),
                &settings,
                None
            ),
            Some("GrasslandGrain".to_string())
        );
        // Undiscovered deposit → plain art until a prospector reveals it.
        assert_eq!(
            terrain_motif_name(
                &tile_with_resource("Mountain", "Gold", true),
                &settings,
                None
            ),
            Some("Mountain".to_string())
        );
        // Debug reveal shows the integrated art for hidden deposits too.
        let debug = RenderSettings {
            show_hidden_resources: true,
            ..settings
        };
        assert_eq!(
            terrain_motif_name(&tile_with_resource("Mountain", "Gold", true), &debug, None),
            Some("MountainGold".to_string())
        );
        // "Show resources" off → plain terrain art everywhere.
        let plain = RenderSettings {
            show_resources: false,
            ..settings
        };
        assert_eq!(
            terrain_motif_name(&tile_with_resource("Hills", "Coal", false), &plain, None),
            Some("Hills".to_string())
        );
        // Bare grassland stays motif-free; sea never has a motif.
        assert_eq!(
            terrain_motif_name(&tile("Grassland", ""), &settings, None),
            None
        );
        assert_eq!(terrain_motif_name(&tile("Sea", ""), &settings, None), None);
    }

    /// Card #539: mountains at a river's head get the outflow-stream motif
    /// in a deterministic hash-picked variant; mid-course mountains, plain
    /// mountains and non-mountain river tiles stay unchanged.
    #[test]
    fn river_source_mountains_get_stream_motif_variants() {
        let settings = RenderSettings::default();

        // River path: mountain source (0,0) → mountain (1,0) → hills (2,0),
        // plus a dry mountain at (0,2) and a sea tile at (3,0).
        let mut source = tile("Mountain", "");
        source.has_river = true;
        let mut mid = tile("Mountain", "");
        (mid.q, mid.has_river) = (1, true);
        let mut hills = tile("Hills", "");
        (hills.q, hills.has_river) = (2, true);
        let mut dry = tile("Mountain", "");
        dry.r = 2;
        let mut sea = tile("Sea", "");
        sea.q = 3;
        let tiles = vec![source.clone(), mid.clone(), hills, dry.clone(), sea];

        let sources = river_source_coords(&tiles);
        assert_eq!(
            sources,
            HashSet::from([(0, 0)]),
            "only the head of the path is a source"
        );

        // The hash pick is stable and in range.
        let v = river_source_variant(0, 0);
        assert_eq!(v, river_source_variant(0, 0));
        assert!((1..=RIVER_SOURCE_VARIANTS).contains(&v));

        // Source mountain motif gains the River suffix; resource variants
        // compose (MountainGoldRiver<v>); everyone else is untouched.
        assert_eq!(
            terrain_motif_name(&source, &settings, Some(v)),
            Some(format!("MountainRiver{v}"))
        );
        let mut gold_source = tile_with_resource("Mountain", "Gold", false);
        gold_source.has_river = true;
        assert_eq!(
            terrain_motif_name(&gold_source, &settings, Some(v)),
            Some(format!("MountainGoldRiver{v}"))
        );
        assert_eq!(
            terrain_motif_name(&mid, &settings, None),
            Some("Mountain".to_string())
        );
        assert_eq!(
            terrain_motif_name(&dry, &settings, None),
            Some("Mountain".to_string())
        );
        // A non-mountain tile never gets the suffix even if a variant is
        // passed by mistake.
        let mut river_hills = tile("Hills", "");
        river_hills.has_river = true;
        assert_eq!(
            terrain_motif_name(&river_hills, &settings, Some(v)),
            Some("Hills".to_string())
        );
    }

    /// Card #539: every coordinate hashes into a valid variant, and the
    /// variants actually vary across coords ("multiple versions").
    #[test]
    fn river_source_variant_is_bounded_and_varied() {
        let mut seen = HashSet::new();
        for q in -20..20 {
            for r in -20..20 {
                let v = river_source_variant(q, r);
                assert!((1..=RIVER_SOURCE_VARIANTS).contains(&v));
                seen.insert(v);
            }
        }
        assert_eq!(seen.len() as u32, RIVER_SOURCE_VARIANTS);
    }

    /// Card #540: two forest looks — vivid pines for good (Timber) forest,
    /// desaturated scrub otherwise — independent of the display toggle.
    #[test]
    fn forest_keys_vivid_vs_scrub_on_timber() {
        for show_resources in [true, false] {
            let settings = RenderSettings {
                show_resources,
                ..RenderSettings::default()
            };
            assert_eq!(
                terrain_motif_name(
                    &tile_with_resource("Forest", "Timber", false),
                    &settings,
                    None
                ),
                Some("Forest".to_string())
            );
            assert_eq!(
                terrain_motif_name(&tile("Forest", ""), &settings, None),
                Some("ForestScrub".to_string())
            );
        }
    }

    #[test]
    fn scrub_forest_uses_scrub_ground_texture() {
        let icons = icons_with_ground();
        let (_, texture) = tile_fill_spec(
            &tile("Forest", ""),
            MapMode::Terrain,
            &HashMap::new(),
            &icons,
        );
        assert_eq!(texture.expect("scrub ground").0, "ForestScrub");
        let (_, texture) = tile_fill_spec(
            &tile_with_resource("Forest", "Timber", false),
            MapMode::Terrain,
            &HashMap::new(),
            &icons,
        );
        assert_eq!(texture.expect("vivid ground").0, "Forest");
        // Missing scrub texture → vivid forest ground beats a flat fill.
        let vivid_only = IconAssets::for_test(&[("ground", "Forest")]);
        let (_, texture) = tile_fill_spec(
            &tile("Forest", ""),
            MapMode::Terrain,
            &HashMap::new(),
            &vivid_only,
        );
        assert_eq!(texture.expect("fallback ground").0, "Forest");
    }

    #[test]
    fn missing_ground_texture_falls_back_to_flat_fill() {
        let no_icons = IconAssets::for_test(&[]);
        let (land_color, land_texture) = tile_fill_spec(
            &tile("Desert", ""),
            MapMode::Terrain,
            &HashMap::new(),
            &no_icons,
        );
        assert!(land_texture.is_none());
        assert_eq!(land_color, theme::terrain_color("Desert"));

        let (sea_color, sea_texture) = tile_fill_spec(
            &tile("Sea", ""),
            MapMode::Political,
            &HashMap::new(),
            &no_icons,
        );
        assert!(sea_texture.is_none());
        assert_eq!(sea_color, theme::terrain_color("Sea"));
    }
}
