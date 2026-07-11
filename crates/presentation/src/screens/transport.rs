//! Transport screen (F2): the map stays live; a right-hand panel mirrors
//! the web `TransportPanel` — freight capacity, per-resource allocation
//! rows with −/+ steppers, under-demand warnings, military rail capacity,
//! and the food-requirement table. Allocations only queue pending state.
//!
//! Opening the screen switches the map to Terrain mode with the transport
//! network visible (the rails the panel talks about); the previous view is
//! restored on close.

use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, RenderSettings, ViewModels};
use crate::game::vm::TransportVm;
use crate::map::icons::IconAssets;
use crate::map::layers::MapMode;
use crate::map::picking::PickingBlocker;
use crate::screens::common::{icon_label, spawn_icon};
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ScrollProps, TooltipText};

const PANEL_WIDTH: f32 = 300.0;
/// Stepper hit targets (plan: ≥ 26px square at 100% scale).
const STEPPER_SIZE: f32 = 28.0;

#[derive(Component)]
pub struct TransportRoot;

#[derive(Component)]
pub struct TransportContent;

/// −/+ stepper: clicking queues `SetTransportAllocation` one step from the
/// current allocation (five steps with Shift held).
#[derive(Component)]
pub struct TransportAdjust {
    pub resource: String,
    pub delta: i32,
}

/// One-click allocation: food requirements first, then availability.
#[derive(Component)]
pub struct AutoFillButton;

/// Map view saved on entry so closing the screen restores it.
#[derive(Resource, Default)]
pub struct TransportUi {
    saved_view: Option<(MapMode, bool)>,
}

pub fn enter_transport(
    mut commands: Commands,
    theme: Res<Theme>,
    mut ui: ResMut<TransportUi>,
    mut mode: ResMut<MapMode>,
    mut settings: ResMut<RenderSettings>,
) {
    // Show the rail network the panel is about: terrain mode + transport
    // overlay, restored on exit.
    ui.saved_view = Some((*mode, settings.show_transport_network));
    *mode = MapMode::Terrain;
    settings.show_transport_network = true;

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

pub fn exit_transport(
    mut commands: Commands,
    roots: Query<Entity, With<TransportRoot>>,
    mut ui: ResMut<TransportUi>,
    mut mode: ResMut<MapMode>,
    mut settings: ResMut<RenderSettings>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Some((saved_mode, show_network)) = ui.saved_view.take() {
        *mode = saved_mode;
        settings.show_transport_network = show_network;
    }
}

/// Which worker-meal slot a hauled resource feeds, if any.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FoodSlot {
    Grain,
    Fruit,
    Meat,
}

fn food_slot(resource: &str) -> Option<FoodSlot> {
    match resource {
        "Grain" => Some(FoodSlot::Grain),
        "Fruit" => Some(FoodSlot::Fruit),
        "Livestock" | "Fish" => Some(FoodSlot::Meat),
        _ => None,
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

    // CC-2 starvation projection, computed by the application layer with
    // the domain's own meal logic (`TransportVm::starvation`). A row goes
    // red ONLY when workers actually go unfed this turn and its food slot
    // is short.
    let starving = transport
        .starvation
        .map(|s| s.workers_unfed > 0)
        .unwrap_or(false);
    let slot_deficit = |resource: &str| -> u32 {
        let Some(s) = transport.starvation else {
            return 0;
        };
        match food_slot(resource) {
            Some(FoodSlot::Grain) => s.grain_short,
            Some(FoodSlot::Fruit) => s.fruit_short,
            Some(FoodSlot::Meat) => s.meat_short,
            None => 0,
        }
    };

    commands.entity(section).with_children(|content| {
        content.spawn((
            Text::new("Freight Cars"),
            theme.font_bold(14.0),
            TextColor(theme::GOLD),
        ));
        content.spawn((
            Text::new(format!("{remaining} of {cap} cars free")),
            theme.font(12.5),
            TextColor(theme::TEXT),
            TooltipText(
                "Each freight car hauls one unit from your depots to the capital warehouse.".into(),
            ),
            Node {
                margin: UiRect::bottom(Val::Px(6.0)),
                ..default()
            },
        ));

        divider(content);
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                margin: UiRect::bottom(Val::Px(4.0)),
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|header| {
                header.spawn((
                    Text::new("Transport Allocation"),
                    theme.font_bold(13.0),
                    TextColor(theme::TEXT),
                ));
                let autofill = widgets::spawn_button(
                    header,
                    &theme,
                    ButtonProps {
                        label: "Auto-fill".into(),
                        font_size: 11.0,
                        enabled: !observer && cap > 0 && !transport.deliveries.is_empty(),
                        ..default()
                    },
                );
                header.commands().entity(autofill).insert((
                    AutoFillButton,
                    TooltipText(
                        "Allocate freight cars to meet food requirements first \
                         (Grain → Fruit → Meat), then spread the remaining \
                         capacity by depot availability."
                            .into(),
                    ),
                ));
            });
        if transport.deliveries.is_empty() {
            content.spawn((
                Text::new("No resources available"),
                theme.font_italic(12.0),
                TextColor(theme::TEXT_DIM),
            ));
            content.spawn((
                Text::new("Build depots on resource tiles and connect them by rail"),
                theme.font_italic(10.5),
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
            let shortfall = demand_qty.saturating_sub(projected);
            // CC-2: red only for "workers starve this turn"; amber for a
            // shortfall worth fixing; neutral otherwise.
            let alarm = starving && slot_deficit(&delivery.resource) > 0;
            let warn = !alarm && shortfall > 0;
            let can_decrease = allocated > 0 && !observer;
            let can_increase =
                remaining > 0 && cap > 0 && allocated < delivery.available && !observer;

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
                    BackgroundColor(if alarm {
                        Color::srgba(0.86, 0.27, 0.27, 0.10)
                    } else {
                        Color::srgba(1.0, 1.0, 1.0, 0.03)
                    }),
                    BorderColor::all(if alarm {
                        Color::srgba(0.86, 0.27, 0.27, 0.45)
                    } else if warn {
                        Color::srgba(0.85, 0.60, 0.23, 0.45)
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
                            font_size: 13.0,
                            width: Some(Val::Px(STEPPER_SIZE)),
                            enabled: can_decrease,
                            ..default()
                        },
                    );
                    row.commands().entity(minus).insert((
                        TransportAdjust {
                            resource: delivery.resource.clone(),
                            delta: -1,
                        },
                        TooltipText("Haul 1 less (Shift-click: 5 less)".into()),
                    ));
                    row.spawn((
                        Text::new(format!("{allocated}/{}", delivery.available)),
                        theme.font(11.5),
                        TextColor(theme::TEXT),
                        TooltipText(format!(
                            "Hauling {allocated} of {} available at your depots",
                            delivery.available
                        )),
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
                            font_size: 13.0,
                            width: Some(Val::Px(STEPPER_SIZE)),
                            enabled: can_increase,
                            ..default()
                        },
                    );
                    row.commands().entity(plus).insert((
                        TransportAdjust {
                            resource: delivery.resource.clone(),
                            delta: 1,
                        },
                        TooltipText("Haul 1 more (Shift-click: 5 more)".into()),
                    ));
                    if shortfall > 0 {
                        let warn_cell = row
                            .spawn(Node {
                                margin: UiRect::left(Val::Px(4.0)),
                                ..default()
                            })
                            .with_children(|cell| {
                                cell.spawn((
                                    Text::new(format!("{shortfall} short")),
                                    theme.font(10.0),
                                    TextColor(if alarm { theme::ALARM } else { theme::WARN }),
                                ));
                            })
                            .id();
                        row.commands().entity(warn_cell).insert(TooltipText(format!(
                            "Hauling {projected} of the {demand_qty} demanded this turn{}",
                            if alarm {
                                " — workers will go hungry without more food"
                            } else {
                                ""
                            }
                        )));
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
                TooltipText(
                    "Workers eat one food each per turn, split across grain, \
                     fruit, and meat (livestock or fish). Canned food covers \
                     any missing unit."
                        .into(),
                ),
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

/// One-click allocation plan: meet food requirements first (Grain → Fruit →
/// Meat, livestock before fish), then spend the remaining capacity on
/// whatever the depots hold most of.
fn autofill_allocations(transport: &TransportVm) -> Vec<(String, u32)> {
    let cap = transport
        .remote_delivery_capacity
        .unwrap_or(transport.total_capacity);
    let available: HashMap<&str, u32> = transport
        .deliveries
        .iter()
        .map(|d| (d.resource.as_str(), d.available))
        .collect();
    let mut alloc: BTreeMap<String, u32> = transport
        .deliveries
        .iter()
        .map(|d| (d.resource.clone(), 0))
        .collect();
    let mut remaining = cap;

    fn take(
        alloc: &mut BTreeMap<String, u32>,
        available: &HashMap<&str, u32>,
        remaining: &mut u32,
        resource: &str,
        want: u32,
    ) -> u32 {
        let Some(entry) = alloc.get_mut(resource) else {
            return 0;
        };
        let headroom = available
            .get(resource)
            .copied()
            .unwrap_or(0)
            .saturating_sub(*entry);
        let got = want.min(headroom).min(*remaining);
        *entry += got;
        *remaining -= got;
        got
    }

    if let Some(food) = transport
        .food_requirement
        .as_ref()
        .filter(|f| f.workers > 0)
    {
        take(&mut alloc, &available, &mut remaining, "Grain", food.grain);
        take(&mut alloc, &available, &mut remaining, "Fruit", food.fruit);
        let meat = take(
            &mut alloc,
            &available,
            &mut remaining,
            "Livestock",
            food.meat,
        );
        take(
            &mut alloc,
            &available,
            &mut remaining,
            "Fish",
            food.meat.saturating_sub(meat),
        );
    }

    let mut rest: Vec<(&str, u32)> = transport
        .deliveries
        .iter()
        .map(|d| (d.resource.as_str(), d.available))
        .collect();
    rest.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (resource, avail) in rest {
        if remaining == 0 {
            break;
        }
        let already = alloc.get(resource).copied().unwrap_or(0);
        take(
            &mut alloc,
            &available,
            &mut remaining,
            resource,
            avail.saturating_sub(already),
        );
    }

    alloc.into_iter().collect()
}

pub fn handle_transport_buttons(
    mut activations: MessageReader<ButtonActivated>,
    buttons: Query<&TransportAdjust>,
    autofill: Query<(), With<AutoFillButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    vms: Res<ViewModels>,
    mut out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(adjust) = buttons.get(*entity) {
            let Some(transport) = vms.transport.as_ref() else {
                continue;
            };
            let step = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                5
            } else {
                1
            };
            let current = transport
                .allocations
                .iter()
                .find(|a| a.resource == adjust.resource)
                .map(|a| a.units)
                .unwrap_or(0);
            let cap = transport
                .remote_delivery_capacity
                .unwrap_or(transport.total_capacity);
            let total: u32 = transport.allocations.iter().map(|a| a.units).sum();
            let available = transport
                .deliveries
                .iter()
                .find(|d| d.resource == adjust.resource)
                .map(|d| d.available)
                .unwrap_or(0);
            let units = if adjust.delta > 0 {
                // Never queue more than the depots can supply or the
                // remaining capacity can haul.
                current
                    + step
                        .min(cap.saturating_sub(total))
                        .min(available.saturating_sub(current))
            } else {
                current.saturating_sub(step)
            };
            if units != current {
                out.write(GameCommand::SetTransportAllocation {
                    resource: adjust.resource.clone(),
                    units,
                });
            }
        } else if autofill.contains(*entity) {
            let Some(transport) = vms.transport.as_ref() else {
                continue;
            };
            let current: HashMap<&str, u32> = transport
                .allocations
                .iter()
                .map(|a| (a.resource.as_str(), a.units))
                .collect();
            for (resource, units) in autofill_allocations(transport) {
                if current.get(resource.as_str()).copied().unwrap_or(0) != units {
                    out.write(GameCommand::SetTransportAllocation { resource, units });
                }
            }
        }
    }
}
