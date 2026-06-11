//! Transient notifications: write a [`Toast`] message and a panel stacks in
//! the top-right corner, fading out at the end of its TTL.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::theme::{self, Theme};

const TOAST_TTL_SECONDS: f32 = 3.5;
const FADE_SECONDS: f32 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Error,
}

impl ToastKind {
    fn accent(self) -> Color {
        match self {
            ToastKind::Info => theme::GOLD,
            ToastKind::Success => theme::SUCCESS,
            ToastKind::Error => theme::ERROR,
        }
    }
}

/// Fire-and-forget notification request.
#[derive(Message, Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
}

impl Toast {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Info,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Success,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Error,
        }
    }
}

#[derive(Component)]
struct ToastContainer;

#[derive(Component)]
struct ToastItem {
    timer: Timer,
    accent: Color,
}

fn setup_container(mut commands: Commands) {
    commands.spawn((
        ToastContainer,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(52.0),
            right: Val::Px(14.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(8.0),
            ..default()
        },
        GlobalZIndex(450),
        Pickable::IGNORE,
    ));
}

fn spawn_toasts(
    mut toasts: MessageReader<Toast>,
    theme: Res<Theme>,
    containers: Query<Entity, With<ToastContainer>>,
    mut commands: Commands,
) {
    let Ok(container) = containers.single() else {
        return;
    };
    for toast in toasts.read() {
        let accent = toast.kind.accent();
        commands.entity(container).with_children(|stack| {
            stack
                .spawn((
                    ToastItem {
                        timer: Timer::from_seconds(TOAST_TTL_SECONDS, TimerMode::Once),
                        accent,
                    },
                    Node {
                        max_width: Val::Px(340.0),
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)).with_left(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(theme::PANEL_BG),
                    BorderColor::all(accent),
                    Pickable::IGNORE,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new(toast.text.clone()),
                        theme.font(13.0),
                        TextColor(theme::TEXT),
                        Pickable::IGNORE,
                    ));
                });
        });
    }
}

fn tick_toasts(
    time: Res<Time>,
    mut commands: Commands,
    mut items: Query<(
        Entity,
        &mut ToastItem,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut texts: Query<&mut TextColor>,
) {
    for (entity, mut item, mut background, mut border, children) in &mut items {
        item.timer.tick(time.delta());
        if item.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }
        let remaining = item.timer.remaining_secs();
        if remaining < FADE_SECONDS {
            let alpha = (remaining / FADE_SECONDS).clamp(0.0, 1.0);
            background.0 = theme::PANEL_BG.with_alpha(theme::PANEL_BG.alpha() * alpha);
            *border = BorderColor::all(item.accent.with_alpha(alpha));
            for child in children {
                if let Ok(mut color) = texts.get_mut(*child) {
                    color.0 = theme::TEXT.with_alpha(alpha);
                }
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Toast>();
    app.add_systems(Startup, setup_container);
    app.add_systems(Update, (spawn_toasts, tick_toasts));
}
