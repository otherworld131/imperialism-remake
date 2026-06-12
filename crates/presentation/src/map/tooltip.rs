//! Map hover tooltip: opens after 1 s hovering the same hex / navy marker,
//! pins ("sticky") after a further 1.5 s, then absorbs clicks on itself and
//! shows "Click to dismiss". Content mirrors `web/src/components/HexTooltip.tsx`.

use bevy::prelude::*;

use crate::game::resources::{RenderSettings, TileIndex, ViewModels};
use crate::game::vm::{MapTile, NavyMarker};
use crate::map::icons::IconAssets;
use crate::map::navy;
use crate::map::picking::{HoverTarget, PickingBlocker};
use crate::theme::{self, Theme};

const OPEN_DELAY: f32 = 1.0;
const PIN_DELAY: f32 = 1.5;

#[derive(Component)]
pub struct MapTooltipNode;

#[derive(Component)]
pub struct MapTooltipContent;

#[derive(Resource, Default)]
pub struct MapTooltipState {
    hover_key: Option<String>,
    hovered_for: f32,
    open: Option<OpenTooltip>,
}

struct OpenTooltip {
    key: String,
    target: HoverTarget,
    sticky: bool,
    open_for: f32,
    pos: Vec2,
}

pub fn setup_map_tooltip(mut commands: Commands) {
    commands
        .spawn((
            MapTooltipNode,
            Node {
                position_type: PositionType::Absolute,
                max_width: Val::Px(280.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::srgba(31.0 / 255.0, 27.0 / 255.0, 16.0 / 255.0, 0.96)),
            BorderColor::all(Color::srgb_u8(0x5a, 0x50, 0x30)),
            GlobalZIndex(500),
            Visibility::Hidden,
            bevy::picking::Pickable::IGNORE,
        ))
        .with_children(|tooltip| {
            tooltip.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                MapTooltipContent,
            ));
        });
}

fn hover_key(target: &HoverTarget) -> Option<String> {
    match target {
        HoverTarget::None | HoverTarget::Treaty { .. } => None,
        HoverTarget::Hex(q, r) => Some(format!("t:{q},{r}")),
        HoverTarget::Navy(key) => Some(format!("m:{key}")),
    }
}

#[allow(clippy::too_many_arguments)]
/// Hide the tooltip immediately (used when a full-screen overlay opens and
/// the hover/update systems stop running).
pub fn hide_map_tooltip(
    mut node: Query<&mut Visibility, With<MapTooltipNode>>,
    mut state: ResMut<MapTooltipState>,
) {
    *state = MapTooltipState::default();
    for mut visibility in &mut node {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
    }
}

pub fn update_map_tooltip(
    time: Res<Time>,
    target: Res<HoverTarget>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    settings: Res<RenderSettings>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut state: ResMut<MapTooltipState>,
    mut commands: Commands,
    mut node: Query<(&mut Node, &mut Visibility, &mut BorderColor), With<MapTooltipNode>>,
    content: Query<Entity, With<MapTooltipContent>>,
) {
    let key = hover_key(&target);

    // Any click dismisses a pinned tooltip (selection still proceeds).
    if mouse.just_pressed(MouseButton::Left) && state.open.as_ref().is_some_and(|open| open.sticky)
    {
        state.open = None;
        state.hover_key = None;
        state.hovered_for = 0.0;
    }

    if key != state.hover_key {
        state.hover_key = key.clone();
        state.hovered_for = 0.0;
        // Moving away closes a non-sticky tooltip.
        if state.open.as_ref().is_some_and(|open| !open.sticky) {
            state.open = None;
        }
    } else {
        state.hovered_for += time.delta_secs();
    }

    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let mut content_dirty = false;

    if state.open.is_none()
        && let Some(key) = key.clone()
        && state.hovered_for >= OPEN_DELAY
        && let Some(pos) = cursor
    {
        state.open = Some(OpenTooltip {
            key,
            target: target.clone(),
            sticky: false,
            open_for: 0.0,
            pos: pos + Vec2::new(14.0, 14.0),
        });
        content_dirty = true;
    }

    let mut sticky_changed = false;
    if let Some(open) = state.open.as_mut()
        && !open.sticky
    {
        open.open_for += time.delta_secs();
        if open.open_for >= PIN_DELAY && Some(open.key.as_str()) == key.as_deref() {
            open.sticky = true;
            sticky_changed = true;
            content_dirty = true;
        }
    }

    let Ok((mut tooltip_node, mut visibility, mut border)) = node.single_mut() else {
        return;
    };
    let Some(open) = state.open.as_ref() else {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    tooltip_node.left = Val::Px(open.pos.x);
    tooltip_node.top = Val::Px(open.pos.y);
    if *visibility != Visibility::Visible {
        *visibility = Visibility::Visible;
        content_dirty = true;
    }
    if sticky_changed || content_dirty {
        *border = BorderColor::all(if open.sticky {
            Color::srgb_u8(0xff, 0xd9, 0x00)
        } else {
            Color::srgb_u8(0x5a, 0x50, 0x30)
        });
    }

    if !content_dirty {
        return;
    }
    let Ok(content_entity) = content.single() else {
        return;
    };
    commands.entity(content_entity).despawn_children();
    // A pinned tooltip absorbs map picking under it.
    let mut root = commands.entity(content_entity);
    if open.sticky {
        root.insert((Interaction::default(), PickingBlocker));
    } else {
        root.remove::<(Interaction, PickingBlocker)>();
    }

    match &open.target {
        HoverTarget::Hex(q, r) => {
            let tile = vms
                .map
                .as_ref()
                .and_then(|tiles| tiles.get(*index.by_coord.get(&(*q, *r))?));
            if let Some(tile) = tile {
                spawn_tile_content(
                    &mut commands,
                    content_entity,
                    tile,
                    &settings,
                    &theme,
                    icons.as_deref(),
                );
            }
        }
        HoverTarget::Navy(key) => {
            if let Some(marker) = vms
                .navy_markers
                .iter()
                .find(|m| navy::marker_key(m) == *key)
            {
                spawn_marker_content(&mut commands, content_entity, marker, &theme);
            }
        }
        HoverTarget::None | HoverTarget::Treaty { .. } => {}
    }
    if open.sticky {
        line(
            &mut commands,
            content_entity,
            "Click to dismiss",
            theme.font_italic(10.0),
            Color::srgb_u8(0x88, 0x88, 0x88),
        );
    }
}

fn line(commands: &mut Commands, parent: Entity, text: &str, font: TextFont, color: Color) {
    commands.spawn((
        Text::new(text.to_string()),
        font,
        TextColor(color),
        ChildOf(parent),
    ));
}

fn spawn_tile_content(
    commands: &mut Commands,
    parent: Entity,
    tile: &MapTile,
    settings: &RenderSettings,
    theme: &Theme,
    icons: Option<&IconAssets>,
) {
    let show_resource = tile
        .resource
        .as_deref()
        .filter(|_| !tile.resource_hidden || settings.show_hidden_resources);

    // Title row: terrain — [icon] resource.
    let title_row = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let title = match show_resource {
        Some(resource) => format!("{} — {resource}", tile.terrain),
        None => tile.terrain.clone(),
    };
    if let (Some(resource), Some(icons)) = (show_resource, icons)
        && let Some(image) = icons.get("commodities", resource)
    {
        commands.spawn((
            Node {
                width: Val::Px(16.0),
                height: Val::Px(16.0),
                ..default()
            },
            ImageNode::new(image),
            ChildOf(title_row),
        ));
    }
    line(
        commands,
        title_row,
        &title,
        theme.font_bold(13.0),
        theme::TEXT,
    );

    if !tile.province.is_empty() {
        let owner_suffix = if tile.owner.is_empty() {
            String::new()
        } else {
            format!(", Province of {}", tile.owner)
        };
        line(
            commands,
            parent,
            &format!("Province: {}{owner_suffix}", tile.province),
            theme.font(12.0),
            theme::TEXT,
        );
    } else if !tile.owner.is_empty() {
        line(
            commands,
            parent,
            &format!("Owner: {}", tile.owner),
            theme.font(12.0),
            theme::TEXT,
        );
    }
    if show_resource.is_some() {
        line(
            commands,
            parent,
            &format!(
                "Level: {}/{}",
                tile.improvement_level, tile.max_improvement_level
            ),
            theme.font(12.0),
            theme::TEXT,
        );
    }
    if tile.has_river {
        line(
            commands,
            parent,
            "River (+1 Fish)",
            theme.font(12.0),
            theme::TEXT,
        );
    }
    if tile.is_capital {
        line(commands, parent, "• Capital", theme.font(12.0), theme::TEXT);
    }
    if tile.has_railroad {
        line(commands, parent, "Railroad", theme.font(12.0), theme::TEXT);
    }
    if tile.has_port {
        let (text, color) = if tile.port_blockaded {
            ("Port (blockaded)", Color::srgb_u8(0xe6, 0x66, 0x66))
        } else {
            ("Port", theme::TEXT)
        };
        line(commands, parent, text, theme.font(12.0), color);
    }
    if tile.has_depot {
        line(commands, parent, "Depot", theme.font(12.0), theme::TEXT);
    }
    if tile.has_fort {
        line(
            commands,
            parent,
            &format!("Fort L{}", tile.fort_level),
            theme.font(12.0),
            theme::TEXT,
        );
    }
    if let Some(civ) = tile.civilian_on_tile.as_ref() {
        let mut text = format!(
            "{} ({}",
            civ.civ_type,
            if civ.working { "working" } else { "idle" }
        );
        if civ.working && civ.turns_remaining > 0 {
            text.push_str(&format!(", {}t left", civ.turns_remaining));
        }
        if let Some(task) = civ.build_task.as_deref() {
            text.push_str(&format!(", building {task}"));
        }
        text.push(')');
        if !civ.owner.is_empty() && civ.owner != tile.owner {
            text.push_str(&format!(" — {}", civ.owner));
        }
        line(
            commands,
            parent,
            &text,
            theme.font(11.0),
            Color::srgb_u8(0xbb, 0xbb, 0xbb),
        );
    }
    if let Some(composition) = tile.army_composition.as_ref()
        && !composition.is_empty()
    {
        let mut entries: Vec<(&String, &u32)> = composition.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut text = format!(
            "Army: {}",
            entries
                .iter()
                .map(|(t, n)| format!("{n} {t}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if tile.army_firepower > 0.0 {
            text.push_str(&format!(" · {:.1} FP", tile.army_firepower));
        }
        line(
            commands,
            parent,
            &text,
            theme.font(11.0),
            Color::srgb_u8(0xbb, 0xbb, 0xbb),
        );
    }
}

fn spawn_marker_content(
    commands: &mut Commands,
    parent: Entity,
    marker: &NavyMarker,
    theme: &Theme,
) {
    let (title, color) = if marker.kind == "beachhead" {
        (
            format!(
                "Beachhead → {}",
                marker.target_province.as_deref().unwrap_or("?")
            ),
            Color::srgb_u8(0xff, 0x80, 0x59),
        )
    } else {
        (format!("Fleet — {}", marker.owner_name), theme::TEXT)
    };
    line(commands, parent, &title, theme.font_bold(13.0), color);
    line(
        commands,
        parent,
        &format!(
            "{} ships · {} FP · {} hull",
            marker.ship_count, marker.total_fp, marker.total_hull
        ),
        theme.font(11.0),
        Color::srgb_u8(0xbb, 0xbb, 0xbb),
    );
    if !marker.by_type.is_empty() {
        let text = marker
            .by_type
            .iter()
            .map(|(t, n)| format!("{n} {t}"))
            .collect::<Vec<_>>()
            .join(", ");
        line(
            commands,
            parent,
            &text,
            theme.font(11.0),
            Color::srgb_u8(0xbb, 0xbb, 0xbb),
        );
    }
    if !marker.by_operation.is_empty() {
        let text = marker
            .by_operation
            .iter()
            .map(|(op, n)| format!("{n} {op}"))
            .collect::<Vec<_>>()
            .join(" · ");
        line(
            commands,
            parent,
            &text,
            theme.font(11.0),
            Color::srgb_u8(0x88, 0x88, 0x88),
        );
    }
}
