//! Checkbox and radio group on top of the headless `bevy_ui_widgets` cores.
//!
//! The cores emit `ValueChange` notifications and leave state management to
//! us: this facade keeps the `Checked` marker in sync, styles the glyphs, and
//! republishes changes as [`CheckboxToggled`] / [`RadioSelected`] messages.

use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{Checked, InteractionDisabled};
use bevy::ui_widgets::{Checkbox as CoreCheckbox, RadioButton, RadioGroup, ValueChange};

use crate::theme::{self, Theme};

// ── Checkbox ────────────────────────────────────────────────────────

/// Marker for checkboxes created by [`spawn_checkbox`].
#[derive(Component)]
pub struct UiCheckbox;

#[derive(Component)]
struct CheckGlyph;

/// Written whenever a kit checkbox flips.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckboxToggled {
    pub entity: Entity,
    pub checked: bool,
}

pub struct CheckboxProps {
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
}

impl Default for CheckboxProps {
    fn default() -> Self {
        Self {
            label: String::new(),
            checked: false,
            enabled: true,
        }
    }
}

pub fn spawn_checkbox(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    props: CheckboxProps,
) -> Entity {
    let mut entity = parent.spawn((
        CoreCheckbox,
        UiCheckbox,
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(2.0)),
            flex_shrink: 0.0,
            ..default()
        },
        Hovered::default(),
    ));
    if props.checked {
        entity.insert(Checked);
    }
    if !props.enabled {
        entity.insert(InteractionDisabled);
    }
    entity.with_children(|row| {
        row.spawn((
            Node {
                width: Val::Px(16.0),
                height: Val::Px(16.0),
                border: UiRect::all(Val::Px(1.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(2.0)),
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(theme::GOLD),
            BackgroundColor(theme::INSET_BG),
            Pickable::IGNORE,
        ))
        .with_children(|boxed| {
            boxed.spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(theme::GOLD),
                if props.checked {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                CheckGlyph,
                Pickable::IGNORE,
            ));
        });
        row.spawn((
            Text::new(props.label),
            theme.font(13.0),
            TextColor(theme::TEXT),
            Pickable::IGNORE,
        ));
    });
    entity.id()
}

/// Apply the core's `ValueChange<bool>` to the `Checked` marker (external
/// state management) and republish as [`CheckboxToggled`].
fn apply_checkbox_change(
    change: On<ValueChange<bool>>,
    checkboxes: Query<(), With<UiCheckbox>>,
    mut commands: Commands,
    mut out: MessageWriter<CheckboxToggled>,
) {
    if !checkboxes.contains(change.source) {
        return;
    }
    if change.value {
        commands.entity(change.source).insert(Checked);
    } else {
        commands.entity(change.source).remove::<Checked>();
    }
    out.write(CheckboxToggled {
        entity: change.source,
        checked: change.value,
    });
}

fn restyle_checkboxes(
    checkboxes: Query<(Has<Checked>, Has<InteractionDisabled>, &Children), With<UiCheckbox>>,
    boxes: Query<&Children, Without<UiCheckbox>>,
    mut glyphs: Query<(&mut Visibility, &mut BackgroundColor), With<CheckGlyph>>,
) {
    for (checked, disabled, children) in &checkboxes {
        for child in children {
            let Ok(grandchildren) = boxes.get(*child) else {
                continue;
            };
            for grandchild in grandchildren {
                if let Ok((mut visibility, mut color)) = glyphs.get_mut(*grandchild) {
                    let target = if checked {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    if *visibility != target {
                        *visibility = target;
                    }
                    let tint = if disabled {
                        theme::TEXT_DIM
                    } else {
                        theme::GOLD
                    };
                    if color.0 != tint {
                        color.0 = tint;
                    }
                }
            }
        }
    }
}

// ── Radio group ─────────────────────────────────────────────────────

/// Marker for radio groups created by [`spawn_radio_group`].
#[derive(Component)]
pub struct UiRadioGroup;

/// Index of a radio option within its group.
#[derive(Component)]
struct UiRadioOption(usize);

#[derive(Component)]
struct RadioDot;

/// Written whenever a kit radio group selection changes.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioSelected {
    pub group: Entity,
    pub index: usize,
}

pub fn spawn_radio_group(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    options: &[&str],
    selected: usize,
) -> Entity {
    parent
        .spawn((
            RadioGroup,
            UiRadioGroup,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|group| {
            for (index, option) in options.iter().enumerate() {
                let mut button = group.spawn((
                    RadioButton,
                    UiRadioOption(index),
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    Hovered::default(),
                ));
                if index == selected {
                    button.insert(Checked);
                }
                button.with_children(|row| {
                    row.spawn((
                        Node {
                            width: Val::Px(16.0),
                            height: Val::Px(16.0),
                            border: UiRect::all(Val::Px(1.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Percent(50.0)),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        BorderColor::all(theme::GOLD),
                        BackgroundColor(theme::INSET_BG),
                        Pickable::IGNORE,
                    ))
                    .with_children(|circle| {
                        circle.spawn((
                            Node {
                                width: Val::Px(8.0),
                                height: Val::Px(8.0),
                                border_radius: BorderRadius::all(Val::Percent(50.0)),
                                ..default()
                            },
                            BackgroundColor(theme::GOLD),
                            if index == selected {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            },
                            RadioDot,
                            Pickable::IGNORE,
                        ));
                    });
                    row.spawn((
                        Text::new(*option),
                        theme.font(13.0),
                        TextColor(theme::TEXT),
                        Pickable::IGNORE,
                    ));
                });
            }
        })
        .id()
}

/// The radio core reports the newly selected button entity on the group; we
/// move the `Checked` marker and republish as [`RadioSelected`].
fn apply_radio_change(
    change: On<ValueChange<Entity>>,
    groups: Query<&Children, With<UiRadioGroup>>,
    options: Query<&UiRadioOption>,
    mut commands: Commands,
    mut out: MessageWriter<RadioSelected>,
) {
    let Ok(children) = groups.get(change.source) else {
        return;
    };
    for child in children {
        if *child == change.value {
            commands.entity(*child).insert(Checked);
        } else {
            commands.entity(*child).remove::<Checked>();
        }
    }
    if let Ok(option) = options.get(change.value) {
        out.write(RadioSelected {
            group: change.source,
            index: option.0,
        });
    }
}

fn restyle_radios(
    buttons: Query<(Has<Checked>, &Children), With<UiRadioOption>>,
    circles: Query<&Children, Without<UiRadioOption>>,
    mut dots: Query<&mut Visibility, With<RadioDot>>,
) {
    for (checked, children) in &buttons {
        for child in children {
            let Ok(grandchildren) = circles.get(*child) else {
                continue;
            };
            for grandchild in grandchildren {
                if let Ok(mut visibility) = dots.get_mut(*grandchild) {
                    let target = if checked {
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
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<CheckboxToggled>();
    app.add_message::<RadioSelected>();
    app.add_observer(apply_checkbox_change);
    app.add_observer(apply_radio_change);
    app.add_systems(Update, (restyle_checkboxes, restyle_radios));
}
