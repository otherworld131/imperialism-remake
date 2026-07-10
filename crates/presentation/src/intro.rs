//! Title screen: the full-screen pixel-art splash (`icons/splash/Title.png`,
//! 320×180 upscaled with nearest-neighbor) with the game title and a
//! "press any key" prompt. Shown as [`AppState::Intro`] before the setup
//! flow; any key or mouse button continues to [`AppState::Setup`]. Debug
//! shortcuts that skip setup (`HUMAN_GAME`, `OBSERVER_GAME`,
//! `MAP_SCREENSHOT`) boot straight into a game and never enter it.

use bevy::prelude::*;

use crate::map::icons::IconAssets;
use crate::state::AppState;
use crate::theme::{self, Theme};

#[derive(Component)]
pub struct IntroRoot;

/// The blinking "press any key" line.
#[derive(Component)]
pub struct IntroPrompt;

/// Spawn the intro UI on the first `Update` frame of the `Intro` state —
/// not `OnEnter`: the initial state transition fires before `Startup`, so
/// `IconAssets` (loaded in `Startup`) would not exist yet.
pub fn setup_intro(
    mut done: Local<bool>,
    mut commands: Commands,
    icons: Res<IconAssets>,
    theme: Res<Theme>,
) {
    if *done {
        return;
    }
    *done = true;
    let mut root = commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(theme::BG),
        GlobalZIndex(50),
        IntroRoot,
    ));
    if let Some(image) = icons.get("splash", "Title") {
        root.insert(ImageNode::new(image));
    }
    root.with_children(|parent| {
        // Title block in the sky region of the splash.
        parent.spawn((
            Text::new("Imperialism"),
            theme.font_bold(96.0),
            TextColor(theme::TEXT),
            TextShadow {
                offset: Vec2::splat(4.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.6),
            },
            Node {
                margin: UiRect::top(Val::Percent(6.0)),
                ..default()
            },
        ));
        parent.spawn((
            Text::new("Remake"),
            theme.font(34.0),
            TextColor(theme::GOLD),
            TextShadow {
                offset: Vec2::splat(2.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.6),
            },
        ));
        // Spacer pushes the prompt to the lower part of the screen.
        parent.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        parent.spawn((
            Text::new("Press any key to begin"),
            theme.font(24.0),
            TextColor(theme::TEXT),
            TextShadow {
                offset: Vec2::splat(2.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.7),
            },
            Node {
                margin: UiRect::bottom(Val::Percent(3.0)),
                ..default()
            },
            IntroPrompt,
        ));
    });
}

/// Gentle blink on the prompt line.
pub fn blink_prompt(time: Res<Time>, mut prompts: Query<&mut TextColor, With<IntroPrompt>>) {
    let alpha = 0.55 + 0.45 * (time.elapsed_secs() * 2.4).sin();
    for mut color in &mut prompts {
        color.0 = color.0.with_alpha(alpha.clamp(0.0, 1.0));
    }
}

/// Any key or mouse button dismisses the intro.
pub fn intro_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some() {
        next_state.set(AppState::Setup);
    }
}

pub fn cleanup_intro(mut commands: Commands, roots: Query<Entity, With<IntroRoot>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}
