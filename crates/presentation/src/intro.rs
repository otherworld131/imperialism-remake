//! Title screen: the full-screen pixel-art splash (`icons/splash/Title.png`,
//! 320×180 upscaled with nearest-neighbor) with the game title and a main
//! menu (New Game / Load Game / Quit). Shown as [`AppState::Intro`] before
//! the setup flow. Enter is a New Game shortcut, Escape quits. Debug
//! shortcuts that skip setup (`HUMAN_GAME`, `OBSERVER_GAME`,
//! `MAP_SCREENSHOT`) boot straight into a game and never enter it.

use bevy::prelude::*;

use crate::map::icons::IconAssets;
use crate::screens::saveload;
use crate::state::AppState;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ModalStack};

#[derive(Component)]
pub struct IntroRoot;

#[derive(Component)]
pub struct IntroNewGameBtn;

#[derive(Component)]
pub struct IntroLoadBtn;

#[derive(Component)]
pub struct IntroQuitBtn;

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
        // Spacer pushes the menu to the lower part of the screen.
        parent.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        // Main menu on a dim plate so the buttons read over the artwork.
        parent
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    margin: UiRect::bottom(Val::Percent(4.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.55)),
            ))
            .with_children(|menu| {
                for (label, width) in [("New Game", 220.0), ("Load Game", 220.0), ("Quit", 220.0)] {
                    let button = widgets::spawn_button(
                        menu,
                        &theme,
                        ButtonProps {
                            label: label.into(),
                            width: Some(Val::Px(width)),
                            font_size: 18.0,
                            ..default()
                        },
                    );
                    let mut commands = menu.commands();
                    let mut entity = commands.entity(button);
                    match label {
                        "New Game" => entity.insert(IntroNewGameBtn),
                        "Load Game" => entity.insert(IntroLoadBtn),
                        _ => entity.insert(IntroQuitBtn),
                    };
                }
            });
    });
}

/// Menu clicks: New Game / Load Game (opens the shared load modal over the
/// setup flow) / Quit.
pub fn intro_menu(
    mut activations: MessageReader<ButtonActivated>,
    new_game: Query<(), With<IntroNewGameBtn>>,
    load: Query<(), With<IntroLoadBtn>>,
    quit: Query<(), With<IntroQuitBtn>>,
    mut commands: Commands,
    mut stack: ResMut<ModalStack>,
    theme: Res<Theme>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for ButtonActivated(entity) in activations.read() {
        if new_game.get(*entity).is_ok() {
            next_state.set(AppState::Setup);
        } else if load.get(*entity).is_ok() {
            // Stay on the title screen: the modal's confirm path drives the
            // async load job, which installs the session and switches to
            // InGame by itself. Cancelling the modal returns to the menu.
            saveload::open_load_modal(&mut commands, &mut stack, &theme);
        } else if quit.get(*entity).is_ok() {
            exit.write(AppExit::Success);
        }
    }
}

/// Keyboard shortcuts: Enter starts a new game, Escape quits. Inert while
/// a modal (the Load Game dialog) is open — Escape then belongs to the
/// modal kit's close-top-modal handling, not to app exit.
pub fn intro_input(
    keys: Res<ButtonInput<KeyCode>>,
    stack: Res<ModalStack>,
    mut next_state: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !stack.is_empty() {
        return;
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        next_state.set(AppState::Setup);
    } else if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

pub fn cleanup_intro(mut commands: Commands, roots: Query<Entity, With<IntroRoot>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}
