//! Styled push button on top of the headless `bevy_ui_widgets::Button` core.
//!
//! The core supplies press tracking (`Pressed`), keyboard activation, and the
//! `Activate` notification; this facade adds the parchment styling
//! (hover/pressed tints, disabled dimming) and converts activations into the
//! [`ButtonActivated`] message so consumers never see the experimental API.

use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Activate, Button as CoreButton};

use crate::theme::{self, Theme};

/// Marker + style flags for buttons created by [`spawn_button`].
#[derive(Component)]
pub struct UiButton {
    flat: bool,
    auto_label_tint: bool,
    /// The border the disabled style displaced, restored on re-enable so
    /// custom border colors round-trip unchanged. Mutated in place (not a
    /// separate component) so restyling never queues commands that could
    /// target an entity despawned in the same frame.
    saved_border: Option<BorderColor>,
}

#[derive(Component)]
struct UiButtonLabel;

/// Written whenever a kit button is clicked or keyboard-activated.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonActivated(pub Entity);

pub struct ButtonProps {
    pub label: String,
    pub width: Option<Val>,
    pub font_size: f32,
    pub enabled: bool,
    /// Borderless, transparent-at-rest variant for rows inside
    /// dropdowns/tabs/table headers.
    pub flat: bool,
    /// When false the kit leaves the label color alone so the caller can
    /// tint it (e.g. the active tab).
    pub auto_label_tint: bool,
}

impl Default for ButtonProps {
    fn default() -> Self {
        Self {
            label: String::new(),
            width: None,
            font_size: 13.0,
            enabled: true,
            flat: false,
            auto_label_tint: true,
        }
    }
}

impl ButtonProps {
    pub fn label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..default()
        }
    }
}

pub fn spawn_button(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    props: ButtonProps,
) -> Entity {
    let mut node = Node {
        height: Val::Px(30.0),
        padding: UiRect::horizontal(Val::Px(12.0)),
        justify_content: if props.flat {
            JustifyContent::FlexStart
        } else {
            JustifyContent::Center
        },
        align_items: AlignItems::Center,
        border: UiRect::all(Val::Px(if props.flat { 0.0 } else { 1.0 })),
        border_radius: BorderRadius::all(Val::Px(3.0)),
        flex_shrink: 0.0,
        ..default()
    };
    if let Some(width) = props.width {
        node.width = width;
    }
    let mut entity = parent.spawn((
        CoreButton,
        UiButton {
            flat: props.flat,
            auto_label_tint: props.auto_label_tint,
            saved_border: None,
        },
        node,
        BorderColor::all(theme::GOLD),
        BackgroundColor(if props.flat {
            Color::NONE
        } else {
            theme::BUTTON_BG
        }),
        Hovered::default(),
    ));
    if !props.enabled {
        entity.insert(InteractionDisabled);
    }
    entity.with_children(|button| {
        button.spawn((
            Text::new(props.label),
            theme.font_bold(props.font_size),
            TextColor(theme::TEXT),
            UiButtonLabel,
            Pickable::IGNORE,
        ));
    });
    entity.id()
}

/// Bridge the experimental `Activate` notification to our public message.
fn forward_activations(
    activate: On<Activate>,
    buttons: Query<(), With<UiButton>>,
    mut out: MessageWriter<ButtonActivated>,
) {
    if buttons.contains(activate.entity) {
        out.write(ButtonActivated(activate.entity));
    }
}

/// Hover/pressed/disabled tinting for background and label. Assignments are
/// compare-guarded so render-side change detection only fires on transitions.
fn restyle_buttons(
    mut buttons: Query<(
        &mut UiButton,
        &Hovered,
        Has<Pressed>,
        Has<InteractionDisabled>,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut labels: Query<&mut TextColor, With<UiButtonLabel>>,
) {
    for (mut button, hovered, pressed, disabled, mut background, mut border, children) in
        &mut buttons
    {
        let rest = if button.flat {
            Color::NONE
        } else {
            theme::BUTTON_BG
        };
        // Disabled is unmistakable: desaturated fill + muted border + muted
        // label, vs the bright gold border every enabled button keeps.
        let (bg, fg) = if disabled {
            (
                if button.flat {
                    Color::NONE
                } else {
                    theme::BUTTON_BG_DISABLED
                },
                theme::TEXT_DISABLED,
            )
        } else if pressed {
            (theme::BUTTON_BG_PRESSED, theme::GOLD)
        } else if hovered.get() {
            (theme::BUTTON_BG_HOVER, theme::TEXT)
        } else {
            (rest, theme::TEXT)
        };
        if background.0 != bg {
            background.0 = bg;
        }
        // Border: stash the current border when the disabled style claims
        // it, and restore that exact border on re-enable — custom colors
        // (war-red confirm, tech-green picks) round-trip unchanged.
        if !button.flat {
            let muted = BorderColor::all(theme::BUTTON_BORDER_DISABLED);
            if disabled && *border != muted {
                button.saved_border = Some(*border);
                *border = muted;
            } else if !disabled && let Some(original) = button.saved_border.take() {
                *border = original;
            }
        }
        if !button.auto_label_tint {
            continue;
        }
        for child in children {
            if let Ok(mut color) = labels.get_mut(*child)
                && color.0 != fg
            {
                color.0 = fg;
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<ButtonActivated>();
    app.add_observer(forward_activations);
    app.add_systems(Update, restyle_buttons);
}
