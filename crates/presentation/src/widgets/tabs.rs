//! Tab row driving child-panel visibility.
//!
//! [`spawn_tabs`] builds a row of flat buttons plus one (initially empty)
//! panel per tab; callers fill the returned panel entities. The active tab is
//! stored in [`TabGroup`]; switching toggles `Display` on the panels.

use bevy::prelude::*;

use super::button::{ButtonActivated, ButtonProps, UiButton, spawn_button};
use crate::theme::{self, Theme};

/// Root state: which tab is active and the panel entity per tab.
#[derive(Component)]
pub struct TabGroup {
    pub active: usize,
    pub panels: Vec<Entity>,
}

#[derive(Component)]
struct TabButton {
    group: Entity,
    index: usize,
}

/// Written when the active tab changes.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabChanged {
    pub group: Entity,
    pub index: usize,
}

pub struct TabsHandles {
    pub root: Entity,
    /// One content container per tab, in label order.
    pub panels: Vec<Entity>,
}

pub fn spawn_tabs(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    labels: &[&str],
    active: usize,
) -> TabsHandles {
    let active = active.min(labels.len().saturating_sub(1));
    let root = parent
        .spawn((Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            ..default()
        },))
        .id();
    let mut panels = Vec::with_capacity(labels.len());
    parent.commands().entity(root).with_children(|tabs| {
        tabs.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(2.0),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(theme::BORDER),
        ))
        .with_children(|row| {
            for (index, label) in labels.iter().enumerate() {
                let button = spawn_button(
                    row,
                    theme,
                    ButtonProps {
                        label: (*label).to_string(),
                        flat: true,
                        auto_label_tint: false,
                        ..default()
                    },
                );
                row.commands()
                    .entity(button)
                    .insert(TabButton { group: root, index });
            }
        });
        for index in 0..labels.len() {
            let panel = tabs
                .spawn((Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(8.0)),
                    display: if index == active {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    ..default()
                },))
                .id();
            panels.push(panel);
        }
    });
    parent.commands().entity(root).insert(TabGroup {
        active,
        panels: panels.clone(),
    });
    TabsHandles { root, panels }
}

fn handle_tab_buttons(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<&TabButton>,
    mut groups: Query<&mut TabGroup>,
    mut changed: MessageWriter<TabChanged>,
) {
    for ButtonActivated(entity) in activations.read() {
        let Ok(button) = buttons.get(*entity) else {
            continue;
        };
        let Ok(mut group) = groups.get_mut(button.group) else {
            continue;
        };
        if group.active != button.index {
            group.active = button.index;
            changed.write(TabChanged {
                group: button.group,
                index: button.index,
            });
        }
    }
}

fn sync_tabs(
    groups: Query<(Entity, &TabGroup), Changed<TabGroup>>,
    mut panels: Query<&mut Node>,
    tab_buttons: Query<(&TabButton, &Children), With<UiButton>>,
    mut labels: Query<&mut TextColor>,
) {
    for (group_entity, group) in &groups {
        for (index, panel) in group.panels.iter().enumerate() {
            if let Ok(mut node) = panels.get_mut(*panel) {
                let display = if index == group.active {
                    Display::Flex
                } else {
                    Display::None
                };
                if node.display != display {
                    node.display = display;
                }
            }
        }
        for (button, children) in &tab_buttons {
            if button.group != group_entity {
                continue;
            }
            let color = if button.index == group.active {
                theme::GOLD
            } else {
                theme::TEXT_DIM
            };
            for child in children {
                if let Ok(mut text_color) = labels.get_mut(*child)
                    && text_color.0 != color
                {
                    text_color.0 = color;
                }
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<TabChanged>();
    app.add_systems(Update, (handle_tab_buttons, sync_tabs).chain());
}
