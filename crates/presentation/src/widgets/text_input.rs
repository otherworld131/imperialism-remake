//! Minimal single-line text input: click to focus (`bevy_input_focus`),
//! character/Backspace/Delete/Home/End/arrow handling, a blinking caret, and
//! a max length. No selection, no IME.
//!
//! The caret is a real node sitting between two `Text` spans (the value
//! split at the caret), so its position always matches glyph layout without
//! any font metrics math.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::InputFocus;
use bevy::picking::Pickable;
use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;

use crate::theme::{self, Theme};

const CARET_BLINK_SECONDS: f32 = 1.1;

/// Single-line input state. `caret` is a char index into `value`.
#[derive(Component)]
pub struct UiTextInput {
    pub value: String,
    pub caret: usize,
    pub max_len: usize,
}

#[derive(Component)]
struct TextBeforeCaret;

/// Dim placeholder text; visible only while the value is empty.
#[derive(Component)]
struct PlaceholderNode;

#[derive(Component)]
struct TextAfterCaret;

#[derive(Component)]
struct CaretNode;

/// Written on every edit.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct TextInputChanged {
    pub entity: Entity,
    pub value: String,
}

pub struct TextInputProps {
    pub width: Val,
    pub max_len: usize,
    pub value: String,
    /// Dim hint shown while the value is empty (e.g. "Filter…").
    pub placeholder: String,
}

impl Default for TextInputProps {
    fn default() -> Self {
        Self {
            width: Val::Px(220.0),
            max_len: 64,
            value: String::new(),
            placeholder: String::new(),
        }
    }
}

pub fn spawn_text_input(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    props: TextInputProps,
) -> Entity {
    let mut value = props.value;
    truncate_chars(&mut value, props.max_len);
    let caret = value.chars().count();
    let entity = parent
        .spawn((
            UiTextInput {
                value: value.clone(),
                caret,
                max_len: props.max_len,
            },
            Node {
                width: props.width,
                height: Val::Px(28.0),
                padding: UiRect::horizontal(Val::Px(8.0)),
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                overflow: Overflow::hidden_x(),
                flex_direction: FlexDirection::Row,
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(theme::INSET_BG),
            BorderColor::all(theme::BORDER),
            Hovered::default(),
        ))
        .with_children(|input| {
            if !props.placeholder.is_empty() {
                input.spawn((
                    Text::new(props.placeholder.clone()),
                    theme.font(13.0),
                    TextColor(theme::TEXT_DIM),
                    PlaceholderNode,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(8.0),
                        ..default()
                    },
                    if value.is_empty() {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    Pickable::IGNORE,
                ));
            }
            input.spawn((
                Text::new(value),
                theme.font(13.0),
                TextColor(theme::TEXT),
                TextBeforeCaret,
                Pickable::IGNORE,
            ));
            input.spawn((
                CaretNode,
                Node {
                    width: Val::Px(1.5),
                    height: Val::Px(16.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(theme::GOLD),
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
            input.spawn((
                Text::new(""),
                theme.font(13.0),
                TextColor(theme::TEXT),
                TextAfterCaret,
                Pickable::IGNORE,
            ));
        })
        .id();
    parent.commands().entity(entity).observe(
        move |_click: On<Pointer<Click>>, mut focus: ResMut<InputFocus>| {
            focus.set(entity);
        },
    );
    entity
}

fn truncate_chars(value: &mut String, max_chars: usize) {
    if let Some((index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(index);
    }
}

fn byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

fn handle_keyboard(
    focus: Res<InputFocus>,
    mut keys: MessageReader<KeyboardInput>,
    mut inputs: Query<&mut UiTextInput>,
    mut changed: MessageWriter<TextInputChanged>,
) {
    let Some(focused) = focus.0 else {
        keys.clear();
        return;
    };
    let Ok(mut input) = inputs.get_mut(focused) else {
        keys.clear();
        return;
    };
    let mut edited = false;
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Character(typed) => {
                for ch in typed.chars().filter(|c| !c.is_control()) {
                    if input.value.chars().count() >= input.max_len {
                        break;
                    }
                    let at = byte_index(&input.value, input.caret);
                    input.value.insert(at, ch);
                    input.caret += 1;
                    edited = true;
                }
            }
            Key::Space => {
                if input.value.chars().count() < input.max_len {
                    let at = byte_index(&input.value, input.caret);
                    input.value.insert(at, ' ');
                    input.caret += 1;
                    edited = true;
                }
            }
            Key::Backspace => {
                if input.caret > 0 {
                    let at = byte_index(&input.value, input.caret - 1);
                    input.value.remove(at);
                    input.caret -= 1;
                    edited = true;
                }
            }
            Key::Delete => {
                if input.caret < input.value.chars().count() {
                    let at = byte_index(&input.value, input.caret);
                    input.value.remove(at);
                    edited = true;
                }
            }
            Key::Home => {
                input.caret = 0;
            }
            Key::End => {
                input.caret = input.value.chars().count();
            }
            Key::ArrowLeft => {
                input.caret = input.caret.saturating_sub(1);
            }
            Key::ArrowRight => {
                input.caret = (input.caret + 1).min(input.value.chars().count());
            }
            _ => {}
        }
    }
    if edited {
        changed.write(TextInputChanged {
            entity: focused,
            value: input.value.clone(),
        });
    }
}

/// Clicking outside the focused input releases focus.
fn release_focus_on_outside_click(
    mouse: Res<ButtonInput<MouseButton>>,
    mut focus: ResMut<InputFocus>,
    inputs: Query<&Hovered, With<UiTextInput>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(focused) = focus.0 else {
        return;
    };
    if let Ok(hovered) = inputs.get(focused)
        && !hovered.get()
    {
        focus.clear();
    }
}

fn sync_text_input_visuals(
    inputs: Query<(Entity, Ref<UiTextInput>, &Children)>,
    focus: Res<InputFocus>,
    mut texts: Query<&mut Text>,
    before: Query<(), With<TextBeforeCaret>>,
    after: Query<(), With<TextAfterCaret>>,
    mut placeholders: Query<&mut Visibility, With<PlaceholderNode>>,
    mut borders: Query<&mut BorderColor, With<UiTextInput>>,
) {
    for (entity, input, children) in &inputs {
        let focused = focus.0 == Some(entity);
        if let Ok(mut border) = borders.get_mut(entity) {
            let target = BorderColor::all(if focused { theme::GOLD } else { theme::BORDER });
            if *border != target {
                *border = target;
            }
        }
        if !input.is_changed() && !focus.is_changed() {
            continue;
        }
        let split = byte_index(&input.value, input.caret);
        for child in children {
            if let Ok(mut visibility) = placeholders.get_mut(*child) {
                let target = if input.value.is_empty() && !focused {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *visibility != target {
                    *visibility = target;
                }
            }
            if before.contains(*child) {
                if let Ok(mut text) = texts.get_mut(*child) {
                    let target = &input.value[..split];
                    if **text != target {
                        **text = target.to_string();
                    }
                }
            } else if after.contains(*child)
                && let Ok(mut text) = texts.get_mut(*child)
            {
                let target = &input.value[split..];
                if **text != target {
                    **text = target.to_string();
                }
            }
        }
    }
}

fn blink_caret(
    time: Res<Time>,
    mut clock: Local<f32>,
    focus: Res<InputFocus>,
    inputs: Query<(Entity, &Children), With<UiTextInput>>,
    mut carets: Query<&mut Visibility, With<CaretNode>>,
) {
    *clock += time.delta_secs();
    let on = (*clock % CARET_BLINK_SECONDS) < CARET_BLINK_SECONDS * 0.6;
    for (entity, children) in &inputs {
        let visible = focus.0 == Some(entity) && on;
        for child in children {
            if let Ok(mut visibility) = carets.get_mut(*child) {
                let target = if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *visibility != target {
                    *visibility = target;
                }
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<TextInputChanged>();
    app.add_systems(
        Update,
        (
            release_focus_on_outside_click,
            handle_keyboard,
            sync_text_input_visuals,
            blink_caret,
        )
            .chain(),
    );
}
