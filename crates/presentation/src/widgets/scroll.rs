//! Scroll container: native bevy_ui overflow scrolling + mouse wheel +
//! a styled scrollbar driven by the headless `bevy_ui_widgets::Scrollbar`
//! core (track paging, thumb drag, and thumb sizing all come from the core).

use bevy::picking::events::{Pointer, Scroll as PointerScroll};
use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, CoreScrollbarThumb, Scrollbar};

use crate::theme::{self, Theme};

/// Pixels per mouse-wheel line.
const LINE_SCROLL_PX: f32 = 28.0;

/// Marker for the scrollable viewport node (the one with `ScrollPosition`).
#[derive(Component)]
pub struct UiScrollArea;

pub struct ScrollProps {
    pub width: Val,
    pub height: Val,
    /// Lets the container stretch inside a flex column (e.g. `1.0` to fill).
    pub flex_grow: f32,
}

impl Default for ScrollProps {
    fn default() -> Self {
        Self {
            width: Val::Percent(100.0),
            height: Val::Auto,
            flex_grow: 0.0,
        }
    }
}

pub struct ScrollHandles {
    pub root: Entity,
    /// Spawn scrollable content as children of this entity.
    pub content: Entity,
}

pub fn spawn_scroll_area(
    parent: &mut ChildSpawnerCommands,
    _theme: &Theme,
    props: ScrollProps,
) -> ScrollHandles {
    let root = parent
        .spawn((Node {
            width: props.width,
            height: props.height,
            flex_grow: props.flex_grow,
            min_height: Val::Px(0.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(2.0),
            ..default()
        },))
        .id();
    let mut viewport = Entity::PLACEHOLDER;
    let mut commands = parent.commands();
    commands.entity(root).with_children(|row| {
        viewport = row
            .spawn((
                UiScrollArea,
                Node {
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    min_height: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
            ))
            .id();
        // Styled scrollbar; the headless core moves/sizes the thumb and
        // handles track clicks + thumb drags.
        row.spawn((
            Scrollbar::new(viewport, ControlOrientation::Vertical, 24.0),
            Node {
                width: Val::Px(8.0),
                height: Val::Percent(100.0),
                flex_shrink: 0.0,
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(theme::INSET_BG),
        ))
        .with_children(|track| {
            track.spawn((
                CoreScrollbarThumb,
                Node {
                    position_type: PositionType::Absolute,
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(theme::GOLD.with_alpha(0.55)),
            ));
        });
    });
    // Mouse wheel: scoped observer so wheel events bubbling up from content
    // reach exactly this viewport.
    commands.entity(viewport).observe(
        move |scroll: On<Pointer<PointerScroll>>, mut areas: Query<&mut ScrollPosition>| {
            if let Ok(mut position) = areas.get_mut(viewport) {
                let delta = match scroll.event.unit {
                    bevy::input::mouse::MouseScrollUnit::Line => scroll.event.y * LINE_SCROLL_PX,
                    bevy::input::mouse::MouseScrollUnit::Pixel => scroll.event.y,
                };
                // Wheel-up (positive y) scrolls toward the top; the layout
                // pass clamps the upper bound.
                position.y = (position.y - delta).max(0.0);
            }
        },
    );
    ScrollHandles {
        root,
        content: viewport,
    }
}

/// This module only contributes the constructor; thumb behavior comes from
/// the core `ScrollbarPlugin` registered by `WidgetsPlugin`.
pub(super) fn plugin(_app: &mut App) {}
