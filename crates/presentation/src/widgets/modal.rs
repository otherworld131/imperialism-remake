//! Modal dialogs: a [`ModalStack`] resource, a full-screen dim layer per
//! modal, a centered parchment panel, and Esc / ✕ popping the top modal.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use super::button::{ButtonActivated, ButtonProps, spawn_button};
use crate::map::picking::PickingBlocker;
use crate::theme::{self, Theme};

/// Open modals, bottom to top. Push via [`open_modal`], pop via
/// [`close_top_modal`] (or Esc / the ✕ button).
#[derive(Resource, Default)]
pub struct ModalStack {
    stack: Vec<Entity>,
}

impl ModalStack {
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

#[derive(Component)]
struct ModalCloseButton(Entity);

pub struct ModalProps {
    pub title: String,
    pub width: Val,
}

impl Default for ModalProps {
    fn default() -> Self {
        Self {
            title: String::new(),
            width: Val::Px(420.0),
        }
    }
}

pub struct ModalHandles {
    /// Full-screen dim layer; despawning it closes the modal.
    pub root: Entity,
    /// Put the dialog body here.
    pub content: Entity,
}

pub fn open_modal(
    commands: &mut Commands,
    stack: &mut ModalStack,
    theme: &Theme,
    props: ModalProps,
) -> ModalHandles {
    let depth = stack.len() as i32;
    let mut content = Entity::PLACEHOLDER;
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme::OVERLAY_BG),
            GlobalZIndex(200 + depth * 10),
            FocusPolicy::Block,
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: props.width,
                        max_height: Val::Percent(85.0),
                        flex_direction: FlexDirection::Column,
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(theme::PANEL_BG_SOLID),
                    BorderColor::all(theme::GOLD),
                ))
                .with_children(|panel| {
                    panel
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                                border: UiRect::bottom(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor::all(theme::BORDER),
                        ))
                        .with_children(|title_bar| {
                            title_bar.spawn((
                                Text::new(props.title.clone()),
                                theme.font_bold(15.0),
                                TextColor(theme::GOLD),
                            ));
                        });
                    content = panel
                        .spawn((Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(14.0)),
                            row_gap: Val::Px(10.0),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },))
                        .id();
                });
        })
        .id();
    // The ✕ button needs the root entity, so it is attached afterwards.
    commands.entity(root).with_children(|overlay| {
        overlay
            .spawn((Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                ..default()
            },))
            .with_children(|corner| {
                let close = spawn_button(
                    corner,
                    theme,
                    ButtonProps {
                        label: "×".into(),
                        flat: true,
                        font_size: 14.0,
                        ..default()
                    },
                );
                corner
                    .commands()
                    .entity(close)
                    .insert(ModalCloseButton(root));
            });
    });
    stack.stack.push(root);
    ModalHandles { root, content }
}

pub fn close_top_modal(commands: &mut Commands, stack: &mut ModalStack) {
    if let Some(root) = stack.stack.pop() {
        commands.entity(root).despawn();
    }
}

/// Esc pops the top modal. The map HUD's "Esc quits" shortcut defers to this
/// by checking [`ModalStack::is_empty`] (and is ordered before this system so
/// the same key press never does both).
pub fn esc_pops_top_modal(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut stack: ResMut<ModalStack>,
) {
    if keys.just_pressed(KeyCode::Escape) && !stack.is_empty() {
        close_top_modal(&mut commands, &mut stack);
    }
}

fn handle_close_buttons(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<&ModalCloseButton>,
    mut commands: Commands,
    mut stack: ResMut<ModalStack>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(close) = buttons.get(*entity) {
            stack.stack.retain(|root| *root != close.0);
            commands.entity(close.0).despawn();
        }
    }
}

/// Drop stack entries whose entities were despawned by other means.
fn prune_stack(mut stack: ResMut<ModalStack>, modals: Query<(), With<Node>>) {
    if stack.stack.iter().any(|root| !modals.contains(*root)) {
        stack.stack.retain(|root| modals.contains(*root));
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<ModalStack>();
    app.add_systems(
        Update,
        (esc_pops_top_modal, handle_close_buttons, prune_stack),
    );
}
