//! Sprite / text marker layers: resource icons with improvement badges,
//! infrastructure, capitals, troop indicators, civilians, navy markers,
//! pending-move arrows, and nation / province / sea-zone labels.
//!
//! Sizing follows HexMap.tsx: every constant is authored in React units
//! (hex 18) and scaled by [`REACT_SCALE`]. All layers live under the wrap
//! roots; LOD-gated groups get a parent entity carrying [`LodGate`].

use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::collections::HashMap;

use crate::game::resources::{
    Blink, PendingMoves, PerspectiveNation, RenderSettings, SelectedCivilian, SelectedNavy,
    TreatyMarkerHit, TreatyMarkerIndex, ViewModels,
};
use crate::game::vm::{DiplomacyRelation, MapTile};
use crate::map::geometry::{self, HEX_SIZE};
use crate::map::icons::IconAssets;
use crate::map::labels::{self, LabelTile};
use crate::map::layers::{MapMode, REACT_SCALE, WrapRoot, react_to_world};
use crate::map::lod::LodGate;
use crate::map::navy;
use crate::map::picking::SelectedHex;
use crate::map::polyline::MeshBuilder2d;
use crate::theme::{self, Theme};

/// React hex size — marker formulas below are written in these units.
const RH: f32 = 18.0;

/// Everything (re)built by [`rebuild_marker_layers`].
#[derive(Component)]
pub struct MarkerLayer;

/// Troop indicator at a capital tile; blinks while its tile is selected.
#[derive(Component)]
pub struct TroopMarker(pub (i32, i32));

/// Steady halo behind a selected troop indicator.
#[derive(Component)]
pub struct TroopHalo(pub (i32, i32));

/// Civilian marker; blinks while its civilian is selected.
#[derive(Component)]
pub struct CivMarker(pub i64);

#[derive(Component)]
pub struct WorkingCivilianAnim {
    phase: f32,
}

/// Three-image pixel animation on a working civilian's icon: while on the
/// job the sprite cycles rest → `<Type>Work1` (mid-swing) → `<Type>Work2`
/// (strike) → mid-swing, i.e. the frames are stored in ping-pong play order.
#[derive(Component)]
pub struct WorkFrameAnim {
    frames: [Handle<Image>; 4],
    phase: f32,
}

#[derive(Component)]
pub struct PendingMoveAnim {
    phase: f32,
    from: Vec2,
    to: Vec2,
}

#[derive(Clone, PartialEq)]
pub struct MarkerKey {
    version: u64,
    mode: MapMode,
    settings: RenderSettings,
    selected_navy: Option<String>,
    pending_moves: usize,
    label_filter: Option<std::collections::BTreeSet<String>>,
}

fn rs(v: f32) -> f32 {
    v * REACT_SCALE
}

/// React-space (y down) point at React units → world Vec2 anchored on a
/// tile center already in world space.
fn world_at(tile_world: Vec2, dx_react: f32, dy_react: f32) -> Vec2 {
    tile_world + Vec2::new(rs(dx_react), -rs(dy_react))
}

struct TextStyle2d {
    font: Handle<Font>,
    size: f32,
    color: Color,
    shadow: Color,
}

fn spawn_outlined_text(
    commands: &mut Commands,
    parent: Entity,
    pos: Vec2,
    z: f32,
    text: &str,
    style: TextStyle2d,
) {
    let offset = rs(0.9);
    commands.spawn((
        Text2d::new(text.to_string()),
        TextFont {
            font: style.font.clone(),
            font_size: style.size,
            ..default()
        },
        TextColor(style.shadow),
        Transform::from_xyz(pos.x + offset, pos.y - offset, z - 0.005),
        ChildOf(parent),
    ));
    commands.spawn((
        Text2d::new(text.to_string()),
        TextFont {
            font: style.font,
            font_size: style.size,
            ..default()
        },
        TextColor(style.color),
        Transform::from_xyz(pos.x, pos.y, z),
        ChildOf(parent),
    ));
}

fn spawn_sprite(
    commands: &mut Commands,
    parent: Entity,
    image: Handle<Image>,
    pos: Vec2,
    z: f32,
    size: f32,
    color: Color,
) -> Entity {
    commands
        .spawn((
            Sprite {
                image,
                custom_size: Some(Vec2::splat(size)),
                color,
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, z),
            ChildOf(parent),
        ))
        .id()
}

fn tent_count(army_unit_count: u32) -> u32 {
    match army_unit_count {
        0 => 0,
        1 => 1,
        2..=4 => 2,
        5..=8 => 3,
        _ => 4,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn rebuild_marker_layers(
    mut commands: Commands,
    vms: Res<ViewModels>,
    mode: Res<MapMode>,
    settings: Res<RenderSettings>,
    selected_navy: Res<SelectedNavy>,
    pending_moves: Res<PendingMoves>,
    perspective: Res<PerspectiveNation>,
    label_filter: Res<crate::game::resources::SessionLabelFilter>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    wrap_roots: Query<Entity, With<WrapRoot>>,
    existing: Query<Entity, With<MarkerLayer>>,
    mut treaty_index: ResMut<TreatyMarkerIndex>,
    mut built: Local<Option<MarkerKey>>,
) {
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };
    if tiles.is_empty() || wrap_roots.is_empty() {
        return;
    }
    let Some(icons) = icons else {
        return;
    };
    let key = MarkerKey {
        version: vms.version,
        mode: *mode,
        settings: *settings,
        selected_navy: selected_navy.0.clone(),
        pending_moves: pending_moves.0.len(),
        label_filter: label_filter.0.clone(),
    };
    if built.as_ref() == Some(&key) && !pending_moves.is_changed() {
        return;
    }
    *built = Some(key);
    let build_started = std::time::Instant::now();

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    treaty_index.0.clear();

    let anchor_indices = navy::anchor_index_map(&vms.navy_markers);

    for root in &wrap_roots {
        let group = |commands: &mut Commands, z: f32, gate: Option<LodGate>| -> Entity {
            let mut entity = commands.spawn((
                MarkerLayer,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::default(),
                ChildOf(root),
            ));
            if let Some(gate) = gate {
                entity.insert(gate);
            }
            entity.id()
        };

        // ── Capitals (gold star icon at country capitals) ───────────────
        {
            let parent = group(&mut commands, 2.0, None);
            if let Some(star) = icons.get("infrastructure", "Capital") {
                for tile in tiles.iter().filter(|t| t.is_country_capital && !t.is_sea()) {
                    let p = geometry::hex_to_world(tile.q, tile.r);
                    spawn_sprite(
                        &mut commands,
                        parent,
                        star.clone(),
                        p,
                        0.0,
                        rs(16.2),
                        Color::WHITE,
                    );
                }
            }
        }

        // ── Resource icons + improvement badges (terrain mode) ──────────
        if settings.show_resources && *mode == MapMode::Terrain {
            let parent = group(&mut commands, 2.1, Some(LodGate::Resources));
            for tile in tiles {
                if tile.is_sea() || tile.owner.is_empty() {
                    continue;
                }
                if tile.is_capital || tile.is_country_capital {
                    continue;
                }
                if tile.resource_hidden && !settings.show_hidden_resources {
                    continue;
                }
                let Some(resource) = tile.resource.as_deref() else {
                    continue;
                };
                let Some(image) = icons.get("commodities", resource) else {
                    continue;
                };
                let p = geometry::hex_to_world(tile.q, tile.r);
                let alpha = if tile.resource_hidden { 0.85 } else { 0.75 };
                spawn_sprite(
                    &mut commands,
                    parent,
                    image,
                    p,
                    0.0,
                    rs(12.6),
                    Color::WHITE.with_alpha(alpha),
                );
                if tile.improvement_level > 0 && tile.max_improvement_level > 0 {
                    let fully = tile.improvement_level >= tile.max_improvement_level;
                    let text = format!("{}/{}", tile.improvement_level, tile.max_improvement_level);
                    spawn_outlined_text(
                        &mut commands,
                        parent,
                        world_at(p, RH * 0.5, RH * 0.55),
                        0.05,
                        &text,
                        TextStyle2d {
                            font: theme.fonts.semibold.clone(),
                            size: rs(7.0),
                            color: if fully {
                                Color::srgb_u8(0xff, 0xd7, 0x00)
                            } else {
                                Color::WHITE
                            },
                            shadow: Color::srgba(0.0, 0.0, 0.0, 0.85),
                        },
                    );
                }
            }
        }

        // ── Infrastructure icons ─────────────────────────────────────────
        {
            let parent = group(&mut commands, 2.2, Some(LodGate::Infra));
            let icon_size = rs(9.0);
            for tile in tiles {
                if tile.is_sea() {
                    continue;
                }
                if !tile.has_depot && !tile.has_port && !tile.has_fort {
                    continue;
                }
                let p = geometry::hex_to_world(tile.q, tile.r);
                if settings.show_transport_network
                    && tile.has_depot
                    && *mode == MapMode::Terrain
                    && let Some(image) = icons.get("infrastructure", "Depot")
                {
                    spawn_sprite(
                        &mut commands,
                        parent,
                        image,
                        world_at(p, RH * 0.3, 0.0),
                        0.0,
                        rs(7.2),
                        Color::WHITE,
                    );
                }
                if settings.show_transport_network
                    && tile.has_port
                    && let Some(image) = icons.get("infrastructure", "Port")
                {
                    let tint = if tile.port_blockaded {
                        Color::srgba(200.0 / 255.0, 40.0 / 255.0, 40.0 / 255.0, 0.95)
                    } else {
                        Color::WHITE
                    };
                    spawn_sprite(
                        &mut commands,
                        parent,
                        image,
                        world_at(p, -RH * 0.3, 0.0),
                        0.01,
                        icon_size,
                        tint,
                    );
                }
                if tile.has_fort
                    && let Some(image) = icons.get("infrastructure", "Fort")
                {
                    let level = tile.fort_level.max(1) as f32;
                    let size = icon_size * (0.8 + level * 0.2);
                    spawn_sprite(
                        &mut commands,
                        parent,
                        image,
                        world_at(p, 0.0, -RH * 0.3),
                        0.02,
                        size,
                        Color::WHITE,
                    );
                }
            }
        }

        // ── Troop encampments at province capitals ───────────────────────
        if settings.show_armies && *mode != MapMode::Diplomatic {
            let parent = group(&mut commands, 3.0, Some(LodGate::Troops));
            let tent = icons.get("ui", "Tent");
            for tile in tiles {
                if tile.is_sea() || !tile.is_capital || tile.army_unit_count == 0 {
                    continue;
                }
                let p = geometry::hex_to_world(tile.q, tile.r);
                let pos = world_at(p, RH * 0.55, -RH * 0.5);
                let tents = tent_count(tile.army_unit_count);
                let tent_size = rs(8.5);

                // Steady selection halo behind the blinking encampment.
                commands.spawn((
                    Mesh2d(meshes.add(Circle::new(rs(11.0)))),
                    MeshMaterial2d(materials.add(Color::srgba(1.0, 220.0 / 255.0, 0.0, 0.35))),
                    Transform::from_xyz(pos.x, pos.y, -0.02),
                    Visibility::Hidden,
                    TroopHalo((tile.q, tile.r)),
                    ChildOf(parent),
                ));

                let marker = commands
                    .spawn((
                        Transform::from_xyz(pos.x, pos.y, 0.0),
                        Visibility::default(),
                        TroopMarker((tile.q, tile.r)),
                        ChildOf(parent),
                    ))
                    .id();
                // Camp layout: tents fan out in a small cluster; closer tents
                // sit lower so the camp reads with a sense of depth. `z` rises
                // toward the front so overlapping tents stack correctly.
                let dx = tent_size * 0.42;
                let dy = tent_size * 0.3;
                let layout: &[(Vec2, f32)] = match tents {
                    1 => &[(Vec2::new(0.0, 0.0), 0.0)],
                    2 => &[
                        (Vec2::new(-dx, dy * 0.4), 0.0),
                        (Vec2::new(dx, -dy * 0.4), 0.02),
                    ],
                    3 => &[
                        (Vec2::new(0.0, dy), 0.0),
                        (Vec2::new(-dx, -dy * 0.7), 0.02),
                        (Vec2::new(dx, -dy * 0.7), 0.02),
                    ],
                    _ => &[
                        (Vec2::new(-dx, dy), 0.0),
                        (Vec2::new(dx, dy), 0.0),
                        (Vec2::new(-dx, -dy), 0.02),
                        (Vec2::new(dx, -dy), 0.02),
                    ],
                };
                if let Some(image) = tent.clone() {
                    for (offset, z) in layout {
                        spawn_sprite(
                            &mut commands,
                            marker,
                            image.clone(),
                            *offset,
                            *z,
                            tent_size,
                            Color::WHITE,
                        );
                    }
                }
                let count_size = rs(6.5);
                spawn_outlined_text(
                    &mut commands,
                    marker,
                    Vec2::new(0.0, -rs(11.5)),
                    0.05,
                    &tile.army_unit_count.to_string(),
                    TextStyle2d {
                        font: theme.fonts.semibold.clone(),
                        size: count_size,
                        color: Color::WHITE,
                        shadow: Color::srgba(0.0, 0.0, 0.0, 0.8),
                    },
                );
            }
        }

        // ── Civilians on tiles (terrain map only) ────────────────────────
        // Civilian workers are an economic-map affordance: on the political
        // and overlay maps they only cluttered the nation fills.
        if *mode == MapMode::Terrain {
            let parent = group(&mut commands, 3.2, Some(LodGate::Civilians));
            let civ_size = rs((RH * 0.9).max(12.0));
            for tile in tiles {
                let Some(civ) = tile.civilian_on_tile.as_ref() else {
                    continue;
                };
                if !civ.is_human && !settings.show_ai_civilians {
                    continue;
                }
                let p = geometry::hex_to_world(tile.q, tile.r);
                let pos = world_at(p, -RH * 0.3, RH * 0.35);
                let marker = commands
                    .spawn((
                        Transform::from_xyz(pos.x, pos.y, 0.0),
                        Visibility::default(),
                        CivMarker(civ.id),
                        ChildOf(parent),
                    ))
                    .id();
                // Nation-colored disc behind the icon.
                if !civ.owner_color.is_empty() {
                    commands.spawn((
                        Mesh2d(meshes.add(Circle::new(civ_size * 0.52))),
                        MeshMaterial2d(
                            materials.add(theme::nation_color(&civ.owner_color).with_alpha(0.58)),
                        ),
                        Transform::from_xyz(0.0, 0.0, -0.01),
                        ChildOf(marker),
                    ));
                }
                if civ.working && civ.turns_remaining > 0 {
                    commands.spawn((
                        Mesh2d(meshes.add(Circle::new(civ_size * 0.68))),
                        MeshMaterial2d(materials.add(Color::srgba(0.2, 0.85, 0.45, 0.45))),
                        Transform::from_xyz(0.0, 0.0, -0.02),
                        WorkingCivilianAnim {
                            phase: (tile.q * 37 + tile.r * 19).rem_euclid(17) as f32 * 0.08,
                        },
                        ChildOf(marker),
                    ));
                }
                if let Some(image) = icons.get("civilians", &civ.civ_type) {
                    let sprite = spawn_sprite(
                        &mut commands,
                        marker,
                        image.clone(),
                        Vec2::ZERO,
                        0.0,
                        civ_size,
                        Color::WHITE,
                    );
                    if civ.working
                        && civ.turns_remaining > 0
                        && let Some(mid) = icons.get("civilians", &format!("{}Work1", civ.civ_type))
                        && let Some(strike) =
                            icons.get("civilians", &format!("{}Work2", civ.civ_type))
                    {
                        commands.entity(sprite).insert(WorkFrameAnim {
                            frames: [image, mid.clone(), strike, mid],
                            phase: (tile.q * 31 + tile.r * 13).rem_euclid(11) as f32 * 0.36,
                        });
                    }
                }
                if civ.working && civ.turns_remaining > 0 {
                    // Engineer build tasks show the target infrastructure
                    // icon to the left (the web uses construction emoji).
                    if let Some(task) = civ.build_task.as_deref()
                        && let Some(image) = icons.get("infrastructure", task)
                    {
                        spawn_sprite(
                            &mut commands,
                            marker,
                            image,
                            Vec2::new(-civ_size * 0.7, 0.0),
                            0.01,
                            civ_size * 0.85,
                            Color::WHITE,
                        );
                    }
                    // Turns-remaining badge, upper right.
                    let badge_pos = Vec2::new(civ_size * 0.5, civ_size * 0.4);
                    commands.spawn((
                        Mesh2d(meshes.add(Circle::new(rs(4.0)))),
                        MeshMaterial2d(materials.add(Color::srgba(0.0, 0.0, 0.0, 0.7))),
                        Transform::from_xyz(badge_pos.x, badge_pos.y, 0.02),
                        ChildOf(marker),
                    ));
                    commands.spawn((
                        Text2d::new(civ.turns_remaining.to_string()),
                        TextFont {
                            font: theme.fonts.semibold.clone(),
                            font_size: rs(5.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        Transform::from_xyz(badge_pos.x, badge_pos.y, 0.03),
                        ChildOf(marker),
                    ));
                }
            }

            // Undeployed civilians fan out around the perspective nation's
            // country capital on a golden-angle spiral.
            if let Some(civs) = vms.civilians.as_ref()
                && !civs.undeployed.is_empty()
                && let Some(capital) = tiles
                    .iter()
                    .find(|t| t.is_country_capital && t.nation_id == i64::from(perspective.0))
            {
                let p = geometry::hex_to_world(capital.q, capital.r);
                for (i, civ) in civs.undeployed.iter().enumerate() {
                    let (dx, dy) = navy::marker_offset(i + 1);
                    let pos = p + Vec2::new(dx * 1.2, -dy * 1.2);
                    if let Some(image) = icons.get("civilians", &civ.civ_type) {
                        let sprite = spawn_sprite(
                            &mut commands,
                            parent,
                            image,
                            pos,
                            0.05,
                            civ_size * 0.78,
                            Color::WHITE.with_alpha(0.95),
                        );
                        commands.entity(sprite).insert(CivMarker(civ.id));
                    }
                }
            }
        }

        // ── Navy markers + beachhead lines + fleet arrows ────────────────
        {
            let parent = group(&mut commands, 3.5, None);
            let radius = navy::NAVY_MARKER_RADIUS;
            let anchor_icon = icons.get("ui", "Anchor");
            let zone_centroids: HashMap<u32, Vec2> = vms
                .sea_zones
                .iter()
                .map(|z| (z.id, geometry::hex_to_world(z.center_q, z.center_r)))
                .collect();
            for marker in &vms.navy_markers {
                let key = navy::marker_key(marker);
                let index = anchor_indices.get(&key).copied().unwrap_or(0);
                let (dx, dy) = navy::marker_offset(index);
                let base = geometry::hex_to_world(marker.q, marker.r);
                let pos = base + Vec2::new(dx, -dy);
                let is_selected = selected_navy.0.as_deref() == Some(key.as_str());

                // Beachhead: thin dashed line toward the coast tile.
                if marker.kind == "beachhead"
                    && let Some(target) = marker.target_hex
                {
                    let mut dash = MeshBuilder2d::default();
                    dash.add_dashed_line(
                        pos,
                        geometry::hex_to_world(target.q, target.r),
                        rs(1.4),
                        rs(2.0),
                        rs(2.0),
                    );
                    commands.spawn((
                        Mesh2d(meshes.add(dash.build())),
                        MeshMaterial2d(materials.add(Color::srgba(
                            230.0 / 255.0,
                            38.0 / 255.0,
                            38.0 / 255.0,
                            0.85,
                        ))),
                        Transform::from_xyz(0.0, 0.0, -0.02),
                        ChildOf(parent),
                    ));
                }

                // Pending fleet move: dashed blue arrow to the target zone.
                if marker.kind == "fleet"
                    && let Some(dest_zone) = marker.pending_move_to_zone_id
                    && let Some(&dest) = zone_centroids.get(&dest_zone)
                {
                    spawn_arrow(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        parent,
                        pos,
                        dest,
                        ArrowStyle {
                            outline: Color::srgba(0.0, 20.0 / 255.0, 60.0 / 255.0, 0.85),
                            fill: Color::srgba(96.0 / 255.0, 168.0 / 255.0, 1.0, 0.95),
                            outline_width: rs(6.0),
                            width: rs(3.0),
                            head_len: rs(16.0),
                            dash: Some((rs(6.0), rs(4.0))),
                        },
                    );
                }

                // Owner-colored disc.
                commands.spawn((
                    Mesh2d(meshes.add(Circle::new(radius))),
                    MeshMaterial2d(materials.add(theme::nation_color(&marker.owner_color))),
                    Transform::from_xyz(pos.x, pos.y, 0.0),
                    ChildOf(parent),
                ));
                // Border ring: red beachhead, gold selected, white otherwise.
                let (ring_color, ring_width) = if marker.kind == "beachhead" {
                    (Color::srgb_u8(0xe6, 0x26, 0x26), rs(1.5))
                } else if is_selected {
                    (Color::srgb_u8(0xff, 0xd9, 0x00), rs(2.5))
                } else {
                    (Color::srgba(1.0, 1.0, 1.0, 0.9), rs(1.5))
                };
                let mut ring = MeshBuilder2d::default();
                ring.add_ring(
                    pos,
                    radius - ring_width / 2.0,
                    radius + ring_width / 2.0,
                    24,
                );
                commands.spawn((
                    Mesh2d(meshes.add(ring.build())),
                    MeshMaterial2d(materials.add(ring_color)),
                    Transform::from_xyz(0.0, 0.0, 0.01),
                    ChildOf(parent),
                ));
                // Anchor glyph.
                if let Some(image) = anchor_icon.clone() {
                    spawn_sprite(
                        &mut commands,
                        parent,
                        image,
                        pos,
                        0.02,
                        rs(12.0),
                        Color::WHITE,
                    );
                }
                // Ship-count badge, top right.
                let badge = pos + Vec2::new(radius - rs(2.0), radius - rs(2.0));
                commands.spawn((
                    Mesh2d(meshes.add(Circle::new(rs(7.0)))),
                    MeshMaterial2d(materials.add(Color::srgb_u8(0x11, 0x11, 0x11))),
                    Transform::from_xyz(badge.x, badge.y, 0.03),
                    ChildOf(parent),
                ));
                commands.spawn((
                    Text2d::new(marker.ship_count.to_string()),
                    TextFont {
                        font: theme.fonts.semibold.clone(),
                        font_size: rs(10.0),
                        ..default()
                    },
                    TextColor(Color::srgb_u8(0xff, 0xd9, 0x00)),
                    Transform::from_xyz(badge.x, badge.y, 0.04),
                    ChildOf(parent),
                ));
            }
        }

        // ── Pending army move arrows (green, solid) ──────────────────────
        if !pending_moves.0.is_empty() {
            let parent = group(&mut commands, 3.8, None);
            let capitals: HashMap<u64, Vec2> = tiles
                .iter()
                .filter(|t| t.is_capital)
                .filter_map(|t| {
                    t.province_id
                        .map(|pid| (pid, geometry::hex_to_world(t.q, t.r)))
                })
                .collect();
            for arrow in &pending_moves.0 {
                let (Some(&from), Some(&to)) = (
                    capitals.get(&arrow.source_province_id),
                    capitals.get(&arrow.dest_province_id),
                ) else {
                    continue;
                };
                spawn_arrow(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    parent,
                    from,
                    to,
                    ArrowStyle {
                        outline: Color::srgba(0.0, 40.0 / 255.0, 0.0, 0.9),
                        fill: Color::srgba(72.0 / 255.0, 220.0 / 255.0, 90.0 / 255.0, 0.95),
                        outline_width: rs(8.0),
                        width: rs(5.0),
                        head_len: rs(18.0),
                        dash: None,
                    },
                );
                let mut dot = MeshBuilder2d::default();
                dot.add_circle(Vec2::ZERO, rs(3.2), 12);
                commands.spawn((
                    Mesh2d(meshes.add(dot.build())),
                    MeshMaterial2d(materials.add(Color::srgba(
                        210.0 / 255.0,
                        1.0,
                        210.0 / 255.0,
                        0.95,
                    ))),
                    Transform::from_xyz(from.x, from.y, 0.04),
                    PendingMoveAnim {
                        phase: ((arrow.source_province_id ^ arrow.dest_province_id) % 11) as f32
                            * 0.09,
                        from,
                        to,
                    },
                    ChildOf(parent),
                ));
            }
        }

        // ── Labels ───────────────────────────────────────────────────────
        spawn_labels(
            &mut commands,
            &theme,
            &vms,
            *mode,
            &icons,
            label_filter.0.as_ref(),
            &mut treaty_index.0,
            root,
            &mut |c, z, gate| {
                let mut entity = c.spawn((
                    MarkerLayer,
                    Transform::from_xyz(0.0, 0.0, z),
                    Visibility::default(),
                    ChildOf(root),
                ));
                if let Some(gate) = gate {
                    entity.insert(gate);
                }
                entity.id()
            },
        );
    }
    info!(
        "map markers rebuild (version {}): {:.1?}",
        vms.version,
        build_started.elapsed(),
    );
}

pub fn animate_map_markers(
    time: Res<Time>,
    mut sets: ParamSet<(
        Query<(
            &WorkingCivilianAnim,
            &mut Transform,
            &mut MeshMaterial2d<ColorMaterial>,
        )>,
        Query<(&PendingMoveAnim, &mut Transform)>,
        Query<(&WorkFrameAnim, &mut Sprite)>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let now = time.elapsed_secs();
    for (anim, mut transform, material) in &mut sets.p0() {
        let wave = ((now * 3.2 + anim.phase) % std::f32::consts::TAU).sin() * 0.5 + 0.5;
        let scale = 0.78 + wave * 0.5;
        transform.scale = Vec3::splat(scale);
        if let Some(mat) = materials.get_mut(&material.0) {
            mat.color = Color::srgba(0.22, 0.9, 0.45, 0.16 + (1.0 - wave) * 0.36);
        }
    }
    for (anim, mut transform) in &mut sets.p1() {
        let t = (now * 0.65 + anim.phase).fract();
        let eased = t * t * (3.0 - 2.0 * t);
        let pos = anim.from.lerp(anim.to, eased);
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
    }
    // Working civilians: step through the ping-pong pose cycle (~0.3s a frame).
    for (anim, mut sprite) in &mut sets.p2() {
        let step = ((now * 3.3 + anim.phase) as usize) % anim.frames.len();
        let target = &anim.frames[step];
        if sprite.image != *target {
            sprite.image = target.clone();
        }
    }
}

/// Diplomacy icon sprites for one relation, in render order (web
/// `diplomacyIconsForRelation`). `(sprite name, action key)`: a key marks the
/// icon as a pending action whose map marker dismisses it on click.
fn diplomacy_icons_for_relation(rel: &DiplomacyRelation) -> Vec<(&'static str, Option<String>)> {
    let mut icons: Vec<(&'static str, Option<String>)> = Vec::new();
    if rel.has_pending_embassy {
        icons.push(("Embassy", Some("embassy".into())));
    } else if rel.has_embassy {
        icons.push(("Embassy", None));
    } else if rel.has_pending_consulate {
        icons.push(("Consulate", Some("consulate".into())));
    } else if rel.has_consulate {
        icons.push(("Consulate", None));
    }

    if rel.has_pending_nap || rel.has_pending_alliance || rel.has_pending_peace {
        let (sprite, key) = if rel.has_pending_peace {
            ("Peace", "peace")
        } else if rel.has_pending_alliance {
            ("Alliance", "alliance")
        } else {
            ("NonAggressionPact", "nap")
        };
        icons.push((sprite, Some(key.into())));
    }

    if let Some(amount) = rel.pending_grant_amount_dollars {
        icons.push(("Grant", Some(format!("grant:{amount}"))));
    }

    for treaty_type in &rel.pending_break_treaties {
        icons.push(("BreakTreaty", Some(format!("break_treaty:{treaty_type}"))));
    }

    if rel.has_pending_war {
        icons.push(("War", Some("war".into())));
    }

    icons
}

struct ArrowStyle {
    outline: Color,
    fill: Color,
    outline_width: f32,
    width: f32,
    head_len: f32,
    dash: Option<(f32, f32)>,
}

fn spawn_arrow(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    parent: Entity,
    from: Vec2,
    to: Vec2,
    style: ArrowStyle,
) {
    let dir = (to - from).normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }
    let shaft_end = to - dir * style.head_len * 0.55;
    let mut outline = MeshBuilder2d::default();
    let mut fill = MeshBuilder2d::default();
    match style.dash {
        Some((dash, gap)) => {
            outline.add_dashed_line(from, shaft_end, style.outline_width, dash, gap);
            fill.add_dashed_line(from, shaft_end, style.width, dash, gap);
        }
        None => {
            outline.add_segment(from, shaft_end, style.outline_width);
            fill.add_segment(from, shaft_end, style.width);
        }
    }
    // Arrowhead: solid in both colors, the fill on top.
    outline.add_arrowhead(to, dir, style.head_len * 1.06, 0.48);
    fill.add_arrowhead(to, dir, style.head_len, 0.45);
    commands.spawn((
        Mesh2d(meshes.add(outline.build())),
        MeshMaterial2d(materials.add(style.outline)),
        Transform::from_xyz(0.0, 0.0, 0.0),
        ChildOf(parent),
    ));
    commands.spawn((
        Mesh2d(meshes.add(fill.build())),
        MeshMaterial2d(materials.add(style.fill)),
        Transform::from_xyz(0.0, 0.0, 0.01),
        ChildOf(parent),
    ));
}

fn spawn_labels(
    commands: &mut Commands,
    theme: &Theme,
    vms: &ViewModels,
    mode: MapMode,
    icons: &IconAssets,
    label_filter: Option<&std::collections::BTreeSet<String>>,
    treaty_hits: &mut Vec<TreatyMarkerHit>,
    _root: Entity,
    group: &mut dyn FnMut(&mut Commands, f32, Option<LodGate>) -> Entity,
) {
    let Some(tiles) = vms.map.as_ref() else {
        return;
    };

    // Sea zone labels: italic, uppercase, always on (subtle alpha).
    if !vms.sea_zones.is_empty() {
        let parent = group(commands, 2.5, Some(LodGate::Grid));
        for zone in &vms.sea_zones {
            if zone.hexes.is_empty() {
                continue;
            }
            let pos = geometry::hex_to_world(zone.center_q, zone.center_r);
            spawn_label_text(
                commands,
                parent,
                pos,
                &zone.name.to_uppercase(),
                TextStyle2d {
                    font: theme.fonts.italic.clone(),
                    size: rs(14.0),
                    color: Color::srgba(200.0 / 255.0, 230.0 / 255.0, 1.0, 0.75),
                    shadow: Color::srgba(0.0, 0.0, 0.0, 0.55),
                },
            );
        }
    }

    // Nation + province labels only exist outside terrain mode
    // (`showPoliticalColors` in the web frontend).
    if mode == MapMode::Terrain {
        return;
    }

    let label_tiles: Vec<LabelTile> = tiles
        .iter()
        .map(|t| LabelTile {
            q: t.q,
            r: t.r,
            is_sea: t.is_sea(),
            owner: t.owner.clone(),
            visual_group: t.visual_group.clone().unwrap_or_default(),
            is_anarchic: t.is_anarchic,
        })
        .collect();
    let nation_labels = labels::compute_nation_labels(&label_tiles, 3, f64::from(HEX_SIZE));

    // ── Diplomatic presence + pending-treaty icons (web pass 5b) ────────
    // Anchored below each nation label centroid in diplomatic mode. Pending
    // icons are clickable (dismiss) — their world hit circles go into
    // `treaty_hits`, filled once per rebuild (the wrap copies share the
    // primary copy's coordinates after wrap-normalization).
    if mode == MapMode::Diplomatic
        && let Some(overlay) = vms.diplomacy.as_ref()
    {
        let parent = group(commands, 3.4, None);
        let fill_hits = treaty_hits.is_empty();
        // React: `Math.max(36, HEX_SIZE * 2.4)` with HEX_SIZE = 18, in
        // React pixel units (converted to world below).
        let emoji_react = 43.2_f64;
        for label in &nation_labels {
            if let Some(filter) = label_filter
                && !filter.contains(&label.name)
            {
                continue;
            }
            let Some(rel) = overlay
                .relations
                .iter()
                .find(|r| r.nation_name == label.name)
            else {
                continue;
            };
            let icon_list = diplomacy_icons_for_relation(rel);
            if icon_list.is_empty() {
                continue;
            }
            let font_size = ((label.size as f32).sqrt() * 3.0).clamp(12.0, 28.0);
            let base_y = label.cy + f64::from(font_size) * 0.6;
            for (i, (sprite_name, action_key)) in icon_list.iter().enumerate() {
                // React anchors the emoji's *top* at y; offset by half the
                // sprite height to match with center-anchored sprites.
                let y = base_y + emoji_react * (0.95 * i as f64 + 0.5);
                let pos = react_to_world([label.cx, y]);
                if let Some(image) = icons.get("diplomacy", sprite_name) {
                    spawn_sprite(
                        commands,
                        parent,
                        image,
                        pos,
                        0.0,
                        rs(emoji_react as f32),
                        Color::WHITE,
                    );
                }
                if fill_hits {
                    treaty_hits.push(TreatyMarkerHit {
                        pos,
                        radius: rs(emoji_react as f32) * 0.8,
                        nation_id: rel.nation_id as u32,
                        action_key: action_key.clone(),
                    });
                }
            }
        }
    }

    let parent = group(commands, 2.6, Some(LodGate::NotPastLabels));
    for label in &nation_labels {
        // Card #494: during the diplomatic session only the two nations
        // involved in the current exchange keep their name on the map.
        if let Some(filter) = label_filter
            && !filter.contains(&label.name)
        {
            continue;
        }
        let size = ((label.size as f32).sqrt() * 3.0).clamp(14.0, 28.0);
        let pos = react_to_world([label.cx, label.cy]);
        let (color, shadow) = if label.is_anarchic {
            (
                Color::srgba(0.0, 0.0, 0.0, 0.95),
                Color::srgba(1.0, 1.0, 1.0, 0.8),
            )
        } else {
            (
                Color::srgba(1.0, 1.0, 1.0, 0.96),
                Color::srgba(0.0, 0.0, 0.0, 0.75),
            )
        };
        spawn_label_text(
            commands,
            parent,
            pos,
            &label.name.to_uppercase(),
            TextStyle2d {
                font: theme.fonts.semibold.clone(),
                size: rs(size),
                color,
                shadow,
            },
        );
    }

    // Province labels (mean centroid per province), zoomed-in only.
    let mut centroids: HashMap<&str, (f64, f64, usize)> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for tile in tiles {
        if tile.is_sea() || tile.province.is_empty() {
            continue;
        }
        let [px, py] = crate::map::borders::hex_to_pixel(tile.q, tile.r, f64::from(HEX_SIZE));
        let entry = centroids.entry(tile.province.as_str()).or_insert_with(|| {
            order.push(tile.province.as_str());
            (0.0, 0.0, 0)
        });
        entry.0 += px;
        entry.1 += py;
        entry.2 += 1;
    }
    let parent = group(commands, 2.6, Some(LodGate::PastLabels));
    for name in order {
        let (sx, sy, count) = centroids[name];
        let size = ((count as f32).sqrt() * 2.5).clamp(9.0, 15.0);
        let pos = react_to_world([sx / count as f64, sy / count as f64]);
        spawn_label_text(
            commands,
            parent,
            pos,
            name,
            TextStyle2d {
                font: theme.fonts.regular.clone(),
                size: rs(size),
                color: Color::srgba(1.0, 0.96, 0.86, 0.98),
                shadow: Color::srgba(0.0, 0.0, 0.0, 0.8),
            },
        );
    }
}

/// Map place-name label root: `0` is the authored font size in world units.
/// [`scale_map_labels`] boosts the whole label (outline included) whenever
/// that size would fall below a readable on-screen height at the current
/// zoom.
#[derive(Component)]
pub struct MapLabelScale(pub f32);

/// Minimum on-screen glyph height (in window pixels) a map label is allowed
/// to render at before [`scale_map_labels`] boosts it.
const MIN_LABEL_SCREEN_PX: f32 = 14.0;
/// Upper bound for the legibility boost, so far-out zooms don't turn labels
/// into map-covering banners.
const MAX_LABEL_BOOST: f32 = 3.0;

fn spawn_label_text(
    commands: &mut Commands,
    parent: Entity,
    pos: Vec2,
    text: &str,
    style: TextStyle2d,
) {
    let offset = (style.size * 0.07).max(1.0);
    let root = commands
        .spawn((
            Transform::from_xyz(pos.x, pos.y, 0.0),
            Visibility::default(),
            MapLabelScale(style.size),
            ChildOf(parent),
        ))
        .id();
    // Full 8-direction outline (not a single drop shadow): place names must
    // stay legible over any terrain/nation fill.
    for (dx, dy) in [
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, 0.0),
        (1.0, 0.0),
    ] {
        commands.spawn((
            Text2d::new(text.to_string()),
            TextFont {
                font: style.font.clone(),
                font_size: style.size,
                ..default()
            },
            TextColor(style.shadow),
            Anchor::CENTER,
            Transform::from_xyz(dx * offset, dy * offset, -0.005),
            ChildOf(root),
        ));
    }
    commands.spawn((
        Text2d::new(text.to_string()),
        TextFont {
            font: style.font,
            font_size: style.size,
            ..default()
        },
        TextColor(style.color),
        Anchor::CENTER,
        Transform::from_xyz(0.0, 0.0, 0.0),
        ChildOf(root),
    ));
}

/// Keep map place names readable at any zoom: when a label's authored world
/// size would render below [`MIN_LABEL_SCREEN_PX`] on screen, scale the whole
/// label root up to compensate (outline offsets scale along with the glyphs).
pub fn scale_map_labels(
    camera: Query<&Projection, With<crate::map::camera::GameCamera>>,
    mut labels: Query<(&MapLabelScale, &mut Transform)>,
) {
    let Ok(Projection::Orthographic(ortho)) = camera.single() else {
        return;
    };
    let ortho_scale = ortho.scale.max(f32::EPSILON);
    for (base, mut transform) in &mut labels {
        let on_screen = base.0 / ortho_scale;
        let boost = if on_screen > 0.0 {
            (MIN_LABEL_SCREEN_PX / on_screen).clamp(1.0, MAX_LABEL_BOOST)
        } else {
            1.0
        };
        if (transform.scale.x - boost).abs() > 1e-3 {
            transform.scale = Vec3::new(boost, boost, 1.0);
        }
    }
}

/// Blink the selected troop indicator / civilian, and show the halo behind
/// a selected capital's troop icon — mirrors the 500 ms web blink.
pub fn blink_selected_markers(
    blink: Res<Blink>,
    selected_hex: Res<SelectedHex>,
    selected_civ: Res<SelectedCivilian>,
    vms: Res<ViewModels>,
    mut sets: ParamSet<(
        Query<(&TroopMarker, &mut Visibility)>,
        Query<(&TroopHalo, &mut Visibility)>,
        Query<(&CivMarker, &mut Visibility)>,
    )>,
) {
    // Selected capital-with-troops, like React's blink trigger.
    let selected_troop_coord = selected_hex.0.filter(|coord| {
        vms.map.as_ref().is_some_and(|tiles| {
            tiles
                .iter()
                .any(|t| (t.q, t.r) == *coord && t.is_capital && t.army_unit_count > 0)
        })
    });
    for (marker, mut visibility) in &mut sets.p0() {
        let selected = selected_troop_coord == Some(marker.0);
        let target = if selected && !blink.on {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != target {
            *visibility = target;
        }
    }
    for (halo, mut visibility) in &mut sets.p1() {
        let target = if selected_troop_coord == Some(halo.0) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != target {
            *visibility = target;
        }
    }
    for (marker, mut visibility) in &mut sets.p2() {
        let selected = selected_civ.0 == Some(marker.0);
        let target = if selected && !blink.on {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != target {
            *visibility = target;
        }
    }
}

/// Tile helper shared by picking + tooltip: troop-indicator hit test in
/// world space (React: within `HEX_SIZE * 0.7` of the indicator anchor).
pub fn troop_indicator_hit(tiles: &[MapTile], world: Vec2) -> Option<(i32, i32)> {
    let hit_radius = rs(RH * 0.7);
    for tile in tiles {
        if !tile.is_capital || tile.army_unit_count == 0 || tile.is_sea() {
            continue;
        }
        let p = geometry::hex_to_world(tile.q, tile.r);
        let anchor = world_at(p, RH * 0.55, -RH * 0.5);
        if anchor.distance_squared(world) <= hit_radius * hit_radius {
            return Some((tile.q, tile.r));
        }
    }
    None
}
