//! Global tooltip: one shared node; any UI entity carrying a
//! [`TooltipText`] component shows it near the cursor after a 0.5 s hover.

use bevy::picking::Pickable;
use bevy::picking::hover::HoverMap;
use bevy::prelude::*;

use crate::theme::{self, Theme};

const HOVER_DELAY_SECONDS: f32 = 0.5;
const CURSOR_OFFSET: Vec2 = Vec2::new(14.0, 18.0);

/// Attach to any UI node to give it a tooltip.
#[derive(Component, Clone)]
pub struct TooltipText(pub String);

#[derive(Component)]
struct TooltipNode;

#[derive(Component)]
struct TooltipLabel;

#[derive(Resource, Default)]
struct TooltipState {
    target: Option<Entity>,
    hovered_for: f32,
}

fn setup_tooltip(mut commands: Commands, theme: Res<Theme>) {
    commands
        .spawn((
            TooltipNode,
            Node {
                position_type: PositionType::Absolute,
                max_width: Val::Px(280.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.04, 0.09, 0.96)),
            BorderColor::all(theme::BORDER),
            GlobalZIndex(600),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|tooltip| {
            tooltip.spawn((
                Text::new(""),
                theme.font(12.5),
                TextColor(theme::TEXT),
                TooltipLabel,
                Pickable::IGNORE,
            ));
        });
}

/// Walk the hovered entity and its ancestors for the nearest tooltip text.
fn hovered_tooltip_target(
    hover_map: &HoverMap,
    tooltips: &Query<&TooltipText>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    for hits in hover_map.values() {
        for start in hits.keys() {
            let mut current = *start;
            loop {
                if tooltips.contains(current) {
                    return Some(current);
                }
                match parents.get(current) {
                    Ok(child_of) => current = child_of.parent(),
                    Err(_) => break,
                }
            }
        }
    }
    None
}

fn update_tooltip(
    time: Res<Time>,
    hover_map: Res<HoverMap>,
    tooltips: Query<&TooltipText>,
    parents: Query<&ChildOf>,
    windows: Query<&Window>,
    mut state: ResMut<TooltipState>,
    mut node: Query<(&mut Node, &mut Visibility), With<TooltipNode>>,
    mut label: Query<&mut Text, With<TooltipLabel>>,
) {
    let target = hovered_tooltip_target(&hover_map, &tooltips, &parents);
    if target != state.target {
        state.target = target;
        state.hovered_for = 0.0;
    } else {
        state.hovered_for += time.delta_secs();
    }

    let Ok((mut tooltip_node, mut visibility)) = node.single_mut() else {
        return;
    };
    let shown = state
        .target
        .filter(|_| state.hovered_for >= HOVER_DELAY_SECONDS);
    let Some(entity) = shown else {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if let Ok(text) = tooltips.get(entity)
        && let Ok(mut label_text) = label.single_mut()
        && **label_text != text.0
    {
        **label_text = text.0.clone();
    }
    let position = (cursor + CURSOR_OFFSET).min(Vec2::new(
        (window.width() - 290.0).max(0.0),
        (window.height() - 60.0).max(0.0),
    ));
    tooltip_node.left = Val::Px(position.x);
    tooltip_node.top = Val::Px(position.y);
    if *visibility != Visibility::Visible {
        *visibility = Visibility::Visible;
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<TooltipState>();
    app.add_systems(Startup, setup_tooltip);
    app.add_systems(Update, update_tooltip);
}
