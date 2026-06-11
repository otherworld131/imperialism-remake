//! Progress bar: a recessed track and a gold fill whose width follows
//! [`UiProgress::fraction`]. Mutate the component to move the bar.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::theme::{self, Theme};

/// Progress state in `0.0..=1.0`.
#[derive(Component)]
pub struct UiProgress {
    pub fraction: f32,
}

#[derive(Component)]
struct ProgressFill;

pub struct ProgressProps {
    pub width: Val,
    pub fraction: f32,
}

impl Default for ProgressProps {
    fn default() -> Self {
        Self {
            width: Val::Px(220.0),
            fraction: 0.0,
        }
    }
}

pub fn spawn_progress(
    parent: &mut ChildSpawnerCommands,
    _theme: &Theme,
    props: ProgressProps,
) -> Entity {
    let fraction = props.fraction.clamp(0.0, 1.0);
    parent
        .spawn((
            UiProgress { fraction },
            Node {
                width: props.width,
                height: Val::Px(14.0),
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(theme::INSET_BG),
            BorderColor::all(theme::BORDER),
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            bar.spawn((
                ProgressFill,
                Node {
                    width: Val::Percent(fraction * 100.0),
                    height: Val::Percent(100.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(theme::GOLD),
                Pickable::IGNORE,
            ));
        })
        .id()
}

fn sync_progress(
    bars: Query<(&UiProgress, &Children), Changed<UiProgress>>,
    mut fills: Query<&mut Node, With<ProgressFill>>,
) {
    for (bar, children) in &bars {
        for child in children {
            if let Ok(mut node) = fills.get_mut(*child) {
                node.width = Val::Percent(bar.fraction.clamp(0.0, 1.0) * 100.0);
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, sync_progress);
}
