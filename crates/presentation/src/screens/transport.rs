//! Transport screen (F2): the map stays live; a right-hand panel mirrors
//! the web `TransportPanel` — freight capacity, per-resource allocation
//! rows with −/+ steppers, under-demand warnings, military rail capacity,
//! and the food-requirement table. Allocations only queue pending state.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, ViewModels};
use crate::map::icons::IconAssets;
use crate::map::picking::PickingBlocker;
use crate::screens::common::{icon_label, spawn_icon};
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ScrollProps, TooltipText};

const PANEL_WIDTH: f32 = 300.0;
const DEMAND_RED: Color = Color::srgb_u8(0xe6, 0x44, 0x44);

#[derive(Component)]
pub struct TransportRoot;

#[derive(Component)]
pub struct TransportContent;

/// −/+ stepper: clicking queues `SetTransportAllocation` with this target.
#[derive(Component)]
pub struct TransportAdjust {
    pub resource: String,
    pub units: u32,
}

pub fn enter_transport(mut commands: Commands, theme: Res<Theme>) {
    commands
        .spawn((
            TransportRoot,
            Node {
                width: Val::Px(PANEL_WIDTH),
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(44.0),
                bottom: Val::Px(0.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            GlobalZIndex(60),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|panel| {
            let scroll = widgets::spawn_scroll_area(
                panel,
                &theme,
                ScrollProps {
                    flex_grow: 1.0,
                    ..default()
                },
            );
            panel
                .commands()
                .entity(scroll.content)
                .insert(TransportContent);
        });
}

pub fn exit_transport(mut commands: Commands, roots: Query<Entity, With<TransportRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

pub fn update_transport(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    sections: Query<Entity, With<TransportContent>>,
    added: Query<(), Added<TransportContent>>,
) {
    if !vms.is_changed() && added.is_empty() {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    let Some(transport) = vms.transport.as_ref() else {
        return;
    };
    let icons = icons.as_deref();
    let observer = meta.observer;

    let allocation: HashMap<&str, u32> = transport
        .allocations
        .iter()
        .map(|a| (a.resource.as_str(), a.units))
        .collect();
    let demand: HashMap<&str, u32> = transport
        .demand
        .iter()
        .map(|d| (d.resource.as_str(), d.demand))
        .collect();
    let cap = transport
        .remote_delivery_capacity
        .unwrap_or(transport.total_capacity);
    let total_allocated: u32 = transport.allocations.iter().map(|a| a.units).sum();
    let remaining = cap.saturating_sub(total_allocated);

    commands.entity(section).with_children(|content| {
        content.spawn((
            Text::new("Freight Cars"),
            theme.font_bold(14.0),
            TextColor(theme::GOLD),
        ));
        content.spawn((
            Text::new(format!("Capacity: {remaining} ({cap})")),
            theme.font(12.5),
            TextColor(theme::TEXT),
            Node {
                margin: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
        ));

        divider(content);
        content.spawn((
            Text::new("Transport Allocation"),
            theme.font_bold(13.0),
            TextColor(theme::TEXT),
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
        ));
        if transport.deliveries.is_empty() {
            content.spawn((
                Text::new("No resources available"),
                theme.font_italic(12.0),
                TextColor(theme::TEXT_DIM),
            ));
        }
        for delivery in &transport.deliveries {
            let allocated = allocation
                .get(delivery.resource.as_str())
                .copied()
                .unwrap_or(0);
            let projected = allocated.min(delivery.available);
            let demand_qty = demand.get(delivery.resource.as_str()).copied().unwrap_or(0);
            let below_demand = demand_qty > 0 && projected < demand_qty;
            let can_decrease = allocated > 0 && !observer;
            let can_increase = remaining > 0 && cap > 0 && !observer;

            content
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                        margin: UiRect::bottom(Val::Px(3.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(if below_demand {
                        Color::srgba(220.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 0.10)
                    } else {
                        Color::srgba(1.0, 1.0, 1.0, 0.03)
                    }),
                    BorderColor::all(if below_demand {
                        Color::srgba(220.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 0.4)
                    } else {
                        Color::NONE
                    }),
                ))
                .with_children(|row| {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        flex_grow: 1.0,
                        min_width: Val::Px(0.0),
                        ..default()
                    })
                    .with_children(|name| {
                        spawn_icon(name, icons, "commodities", &delivery.resource, 14.0);
                        name.spawn((
                            Text::new(delivery.resource.clone()),
                            theme.font(12.5),
                            TextColor(theme::TEXT),
                        ));
                    });
                    let minus = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: "−".into(),
                            font_size: 12.0,
                            width: Some(Val::Px(24.0)),
                            enabled: can_decrease,
                            ..default()
                        },
                    );
                    row.commands().entity(minus).insert(TransportAdjust {
                        resource: delivery.resource.clone(),
                        units: allocated.saturating_sub(1),
                    });
                    row.spawn((
                        Text::new(format!("{allocated}/{}", delivery.available)),
                        theme.font(11.5),
                        TextColor(theme::TEXT),
                        Node {
                            width: Val::Px(42.0),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                    ));
                    let plus = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: "+".into(),
                            font_size: 12.0,
                            width: Some(Val::Px(24.0)),
                            enabled: can_increase,
                            ..default()
                        },
                    );
                    row.commands().entity(plus).insert(TransportAdjust {
                        resource: delivery.resource.clone(),
                        units: allocated + 1,
                    });
                    if below_demand {
                        let warn = row
                            .spawn(Node {
                                margin: UiRect::left(Val::Px(4.0)),
                                ..default()
                            })
                            .with_children(|cell| {
                                cell.spawn((
                                    Text::new(format!("▼{demand_qty}")),
                                    theme.font(10.0),
                                    TextColor(DEMAND_RED),
                                ));
                            })
                            .id();
                        row.commands()
                            .entity(warn)
                            .insert(TooltipText(format!("Demand: {demand_qty}")));
                    }
                });
        }

        divider(content);
        content.spawn((
            Text::new("Military Transport"),
            theme.font_bold(13.0),
            TextColor(theme::TEXT),
        ));
        content.spawn((
            Text::new(format!(
                "Rail capacity: {} unit{}",
                transport.military_transport_capacity,
                if transport.military_transport_capacity == 1 {
                    ""
                } else {
                    "s"
                }
            )),
            theme.font(12.0),
            TextColor(theme::TEXT_DIM),
        ));

        if let Some(food) = transport
            .food_requirement
            .as_ref()
            .filter(|f| f.workers > 0)
        {
            divider(content);
            content.spawn((
                Text::new("Food Requirements"),
                theme.font_bold(13.0),
                TextColor(theme::TEXT),
            ));
            content.spawn((
                Text::new(format!(
                    "{} worker{}",
                    food.workers,
                    if food.workers == 1 { "" } else { "s" }
                )),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
                Node {
                    margin: UiRect::bottom(Val::Px(3.0)),
                    ..default()
                },
            ));
            food_row(content, &theme, icons, "Grain", "Grain", food.grain);
            food_row(content, &theme, icons, "Fruit", "Fruit", food.fruit);
            food_row(content, &theme, icons, "Livestock", "Meat", food.meat);
        }
    });
}

fn divider(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Node {
            width: Val::Percent(100.0),
            border: UiRect::top(Val::Px(1.0)),
            margin: UiRect::vertical(Val::Px(8.0)),
            ..default()
        },
        BorderColor::all(theme::BORDER),
    ));
}

fn food_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    icon: &str,
    label: &str,
    qty: u32,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::vertical(Val::Px(1.0)),
            ..default()
        })
        .with_children(|row| {
            icon_label(
                row,
                theme,
                icons,
                "commodities",
                icon,
                label,
                12.0,
                theme::TEXT_DIM,
            );
            row.spawn((
                Text::new(format!("{qty}")),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
            ));
        });
}

pub fn handle_transport_buttons(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<&TransportAdjust>,
    mut out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(adjust) = buttons.get(*entity) {
            out.write(GameCommand::SetTransportAllocation {
                resource: adjust.resource.clone(),
                units: adjust.units,
            });
        }
    }
}
