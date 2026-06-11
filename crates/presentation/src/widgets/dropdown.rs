//! Single-select dropdown and a multi-select variant with checkbox rows and
//! All/None actions (hand-rolled on the kit's own button/checkbox facades;
//! the experimental menu/popover cores impose an anchoring model we don't
//! need for a simple anchored popup).

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Checked;

use super::button::{ButtonActivated, ButtonProps, UiButton, spawn_button};
use super::checkbox::{CheckboxProps, CheckboxToggled, UiCheckbox, spawn_checkbox};
use crate::map::picking::PickingBlocker;
use crate::theme::{self, Theme};

// ── Single select ───────────────────────────────────────────────────

/// Root state for a single-select dropdown.
#[derive(Component)]
pub struct UiDropdown {
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
}

#[derive(Component)]
struct DropdownHeader(Entity);

#[derive(Component)]
struct DropdownItem {
    root: Entity,
    index: usize,
}

#[derive(Component)]
struct DropdownPopup(Entity);

/// Written when the selection changes.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropdownChanged {
    pub entity: Entity,
    pub index: usize,
}

pub struct DropdownProps {
    pub options: Vec<String>,
    pub selected: usize,
    pub width: Val,
}

impl Default for DropdownProps {
    fn default() -> Self {
        Self {
            options: Vec::new(),
            selected: 0,
            width: Val::Px(200.0),
        }
    }
}

pub fn spawn_dropdown(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    props: DropdownProps,
) -> Entity {
    let selected = props.selected.min(props.options.len().saturating_sub(1));
    let header_label = format!(
        "{}  ▾",
        props.options.get(selected).map_or("", String::as_str)
    );
    let root = parent
        .spawn((
            UiDropdown {
                options: props.options,
                selected,
                open: false,
            },
            Node {
                flex_direction: FlexDirection::Column,
                width: props.width,
                ..default()
            },
            Hovered::default(),
        ))
        .id();
    let mut commands = parent.commands();
    commands.entity(root).with_children(|dropdown| {
        let header = spawn_button(
            dropdown,
            theme,
            ButtonProps {
                label: header_label,
                width: Some(Val::Percent(100.0)),
                ..default()
            },
        );
        dropdown
            .commands()
            .entity(header)
            .insert(DropdownHeader(root));
    });
    root
}

// ── Multi select ────────────────────────────────────────────────────

/// Root state for a multi-select dropdown with checkbox rows.
#[derive(Component)]
pub struct UiMultiDropdown {
    pub label: String,
    pub options: Vec<String>,
    pub selected: Vec<bool>,
    pub open: bool,
}

#[derive(Component)]
struct MultiHeader(Entity);

#[derive(Component)]
struct MultiItem {
    root: Entity,
    index: usize,
}

#[derive(Component)]
struct MultiAll(Entity);

#[derive(Component)]
struct MultiNone(Entity);

/// Written whenever the selected set changes (per row toggle or All/None).
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct MultiDropdownChanged {
    pub entity: Entity,
    pub selected: Vec<bool>,
}

pub struct MultiDropdownProps {
    /// Short noun shown in the header, e.g. "Goods".
    pub label: String,
    pub options: Vec<String>,
    pub selected: Vec<bool>,
    pub width: Val,
}

impl Default for MultiDropdownProps {
    fn default() -> Self {
        Self {
            label: String::new(),
            options: Vec::new(),
            selected: Vec::new(),
            width: Val::Px(220.0),
        }
    }
}

pub fn spawn_multi_dropdown(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    props: MultiDropdownProps,
) -> Entity {
    let mut selected = props.selected;
    selected.resize(props.options.len(), false);
    let header_label = multi_header_label(&props.label, &selected);
    let root = parent
        .spawn((
            UiMultiDropdown {
                label: props.label,
                options: props.options,
                selected,
                open: false,
            },
            Node {
                flex_direction: FlexDirection::Column,
                width: props.width,
                ..default()
            },
            Hovered::default(),
        ))
        .id();
    parent.commands().entity(root).with_children(|dropdown| {
        let header = spawn_button(
            dropdown,
            theme,
            ButtonProps {
                label: header_label,
                width: Some(Val::Percent(100.0)),
                ..default()
            },
        );
        dropdown.commands().entity(header).insert(MultiHeader(root));
    });
    root
}

fn multi_header_label(label: &str, selected: &[bool]) -> String {
    let count = selected.iter().filter(|s| **s).count();
    format!("{label}: {count}/{}  ▾", selected.len())
}

// ── Behavior ────────────────────────────────────────────────────────

fn handle_buttons(
    mut activations: MessageReader<ButtonActivated>,
    headers: Query<&DropdownHeader>,
    items: Query<&DropdownItem>,
    multi_headers: Query<&MultiHeader>,
    all_buttons: Query<&MultiAll>,
    none_buttons: Query<&MultiNone>,
    mut dropdowns: Query<&mut UiDropdown>,
    mut multis: Query<&mut UiMultiDropdown>,
    mut changed: MessageWriter<DropdownChanged>,
    mut multi_changed: MessageWriter<MultiDropdownChanged>,
) {
    for ButtonActivated(button) in activations.read() {
        if let Ok(header) = headers.get(*button)
            && let Ok(mut dropdown) = dropdowns.get_mut(header.0)
        {
            dropdown.open = !dropdown.open;
        }
        if let Ok(item) = items.get(*button)
            && let Ok(mut dropdown) = dropdowns.get_mut(item.root)
        {
            dropdown.open = false;
            if dropdown.selected != item.index {
                dropdown.selected = item.index;
                changed.write(DropdownChanged {
                    entity: item.root,
                    index: item.index,
                });
            }
        }
        if let Ok(header) = multi_headers.get(*button)
            && let Ok(mut multi) = multis.get_mut(header.0)
        {
            multi.open = !multi.open;
        }
        for (query_entity, value) in [
            (all_buttons.get(*button).map(|b| b.0), true),
            (none_buttons.get(*button).map(|b| b.0), false),
        ] {
            if let Ok(root) = query_entity
                && let Ok(mut multi) = multis.get_mut(root)
                && multi.selected.iter().any(|s| *s != value)
            {
                multi.selected.iter_mut().for_each(|s| *s = value);
                multi_changed.write(MultiDropdownChanged {
                    entity: root,
                    selected: multi.selected.clone(),
                });
            }
        }
    }
}

fn handle_multi_checkboxes(
    mut toggles: MessageReader<CheckboxToggled>,
    items: Query<&MultiItem>,
    mut multis: Query<&mut UiMultiDropdown>,
    mut multi_changed: MessageWriter<MultiDropdownChanged>,
) {
    for toggle in toggles.read() {
        let Ok(item) = items.get(toggle.entity) else {
            continue;
        };
        let Ok(mut multi) = multis.get_mut(item.root) else {
            continue;
        };
        if let Some(slot) = multi.selected.get_mut(item.index) {
            *slot = toggle.checked;
        }
        multi_changed.write(MultiDropdownChanged {
            entity: item.root,
            selected: multi.selected.clone(),
        });
    }
}

/// Open/close popups and refresh header labels when state changes.
fn sync_dropdowns(
    theme: Res<Theme>,
    mut commands: Commands,
    dropdowns: Query<(Entity, &UiDropdown, &Children), Changed<UiDropdown>>,
    popups: Query<(Entity, &DropdownPopup)>,
    headers: Query<(), With<DropdownHeader>>,
    buttons: Query<&Children, With<UiButton>>,
    mut labels: Query<&mut Text>,
) {
    for (root, dropdown, children) in &dropdowns {
        // Header label.
        for child in children {
            if headers.contains(*child)
                && let Ok(button_children) = buttons.get(*child)
            {
                for label in button_children {
                    if let Ok(mut text) = labels.get_mut(*label) {
                        **text = format!(
                            "{}  ▾",
                            dropdown
                                .options
                                .get(dropdown.selected)
                                .map_or("", String::as_str)
                        );
                    }
                }
            }
        }
        let existing = popups.iter().find(|(_, p)| p.0 == root).map(|(e, _)| e);
        match (dropdown.open, existing) {
            (false, Some(popup)) => commands.entity(popup).despawn(),
            (true, None) => {
                commands.entity(root).with_children(|parent| {
                    spawn_popup_frame(parent, root, |popup| {
                        for (index, option) in dropdown.options.iter().enumerate() {
                            let row = spawn_button(
                                popup,
                                &theme,
                                ButtonProps {
                                    label: if index == dropdown.selected {
                                        format!("• {option}")
                                    } else {
                                        format!("  {option}")
                                    },
                                    width: Some(Val::Percent(100.0)),
                                    flat: true,
                                    ..default()
                                },
                            );
                            popup
                                .commands()
                                .entity(row)
                                .insert(DropdownItem { root, index });
                        }
                    });
                });
            }
            _ => {}
        }
    }
}

fn sync_multi_dropdowns(
    theme: Res<Theme>,
    mut commands: Commands,
    multis: Query<(Entity, &UiMultiDropdown, &Children), Changed<UiMultiDropdown>>,
    popups: Query<(Entity, &DropdownPopup)>,
    headers: Query<(), With<MultiHeader>>,
    buttons: Query<&Children, With<UiButton>>,
    mut labels: Query<&mut Text>,
    item_boxes: Query<(Entity, &MultiItem), With<UiCheckbox>>,
) {
    for (root, multi, children) in &multis {
        for child in children {
            if headers.contains(*child)
                && let Ok(button_children) = buttons.get(*child)
            {
                for label in button_children {
                    if let Ok(mut text) = labels.get_mut(*label) {
                        **text = multi_header_label(&multi.label, &multi.selected);
                    }
                }
            }
        }
        let existing = popups.iter().find(|(_, p)| p.0 == root).map(|(e, _)| e);
        match (multi.open, existing) {
            (false, Some(popup)) => commands.entity(popup).despawn(),
            (true, None) => {
                commands.entity(root).with_children(|parent| {
                    spawn_popup_frame(parent, root, |popup| {
                        popup
                            .spawn((Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(6.0),
                                margin: UiRect::bottom(Val::Px(4.0)),
                                ..default()
                            },))
                            .with_children(|actions| {
                                let all = spawn_button(
                                    actions,
                                    &theme,
                                    ButtonProps {
                                        label: "All".into(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                );
                                actions.commands().entity(all).insert(MultiAll(root));
                                let none = spawn_button(
                                    actions,
                                    &theme,
                                    ButtonProps {
                                        label: "None".into(),
                                        font_size: 12.0,
                                        ..default()
                                    },
                                );
                                actions.commands().entity(none).insert(MultiNone(root));
                            });
                        for (index, option) in multi.options.iter().enumerate() {
                            let row = spawn_checkbox(
                                popup,
                                &theme,
                                CheckboxProps {
                                    label: option.clone(),
                                    checked: multi.selected.get(index).copied().unwrap_or(false),
                                    enabled: true,
                                },
                            );
                            popup
                                .commands()
                                .entity(row)
                                .insert(MultiItem { root, index });
                        }
                    });
                });
            }
            (true, Some(_)) => {
                // Popup already open (All/None or a row toggle): sync the
                // checkbox rows to the state without rebuilding.
                for (entity, item) in &item_boxes {
                    if item.root != root {
                        continue;
                    }
                    if multi.selected.get(item.index).copied().unwrap_or(false) {
                        commands.entity(entity).insert(Checked);
                    } else {
                        commands.entity(entity).remove::<Checked>();
                    }
                }
            }
            (false, None) => {}
        }
    }
}

fn spawn_popup_frame(
    parent: &mut ChildSpawnerCommands,
    root: Entity,
    fill: impl FnOnce(&mut ChildSpawnerCommands),
) {
    parent
        .spawn((
            DropdownPopup(root),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(100.0),
                left: Val::Px(0.0),
                min_width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                max_height: Val::Px(280.0),
                overflow: Overflow::scroll_y(),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            BorderColor::all(theme::BORDER),
            GlobalZIndex(300),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(fill);
}

/// Clicking anywhere outside an open dropdown closes it.
fn close_on_outside_click(
    mouse: Res<ButtonInput<MouseButton>>,
    mut dropdowns: Query<(&mut UiDropdown, &Hovered)>,
    mut multis: Query<(&mut UiMultiDropdown, &Hovered)>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    for (mut dropdown, hovered) in &mut dropdowns {
        if dropdown.open && !hovered.get() {
            dropdown.open = false;
        }
    }
    for (mut multi, hovered) in &mut multis {
        if multi.open && !hovered.get() {
            multi.open = false;
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<DropdownChanged>();
    app.add_message::<MultiDropdownChanged>();
    app.add_systems(
        Update,
        (
            close_on_outside_click,
            handle_buttons,
            handle_multi_checkboxes,
            (sync_dropdowns, sync_multi_dropdowns),
        )
            .chain(),
    );
}
