//! Industry screen (F3): full-screen overlay mirroring the web
//! `IndustryPanel` — production-chain sliders, labor, education,
//! immigration, buildings, warehouse, logistics, and the army / naval /
//! civilian build queues. Every control merely queues pending state; the
//! end-turn pipeline applies it.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::ui::InteractionDisabled;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, ViewModels};
use crate::game::vm::{BuildableEntryVm, IndustryVm};
use crate::map::icons::IconAssets;
use crate::screens::common::{
    fmt_thousands, icon_label, inset_panel, section_title, spawn_icon, split_camel, unit_icon_name,
};
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, CheckboxProps, CheckboxToggled, SliderCommitted,
    SliderProps, TooltipText, UNLIMITED,
};

const COMMITTED_BLUE: Color = Color::srgb_u8(0x6a, 0xb0, 0xd4);
const TARGET_BEHIND: Color = Color::srgb_u8(0xd9, 0x7a, 0x4a);
const TARGET_MET: Color = Color::srgb_u8(0x66, 0xaa, 0x88);

// ── Markers / state ──────────────────────────────────────────────────────

#[derive(Component)]
pub struct IndustryRoot;

#[derive(Component)]
pub struct IndustryContent;

/// What a committed slider in this screen means for the game.
#[derive(Component, Clone)]
pub enum IndustryAction {
    Chain {
        chain: &'static str,
        step: &'static str,
    },
    TrainToTrained,
    TrainToExpert,
    Immigration,
    FreightCars,
    Recruit(String),
    Ship(String),
    Hire(String),
}

#[derive(Component)]
pub struct ExpandButton(pub String);

#[derive(Component)]
pub struct ShowTargetsCheckbox;

/// Screen-local UI state (web `showTargets` localStorage toggle).
/// AI-target debug data stays hidden until explicitly enabled.
#[derive(Resource, Default)]
pub struct IndustryUi {
    pub show_targets: bool,
}

// ── Lifecycle ────────────────────────────────────────────────────────────

pub fn enter_industry(mut commands: Commands, theme: Res<Theme>) {
    let root = crate::screens::common::full_screen_root(&mut commands);
    commands.entity(root).insert(IndustryRoot);
    commands.entity(root).with_children(|panel| {
        let scroll = widgets::spawn_scroll_area(
            panel,
            &theme,
            widgets::ScrollProps {
                flex_grow: 1.0,
                ..default()
            },
        );
        panel
            .commands()
            .entity(scroll.content)
            .insert(IndustryContent);
    });
}

pub fn exit_industry(mut commands: Commands, roots: Query<Entity, With<IndustryRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

// ── Rebuild ──────────────────────────────────────────────────────────────

pub fn update_industry(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    ui: Res<IndustryUi>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    sections: Query<Entity, With<IndustryContent>>,
    added: Query<(), Added<IndustryContent>>,
) {
    if !vms.is_changed() && !ui.is_changed() && added.is_empty() {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    let Some(industry) = vms.industry.as_ref() else {
        return;
    };
    let icons = icons.as_deref();
    let observer = meta.observer;
    let buildable = vms.buildable.as_ref();
    let committed = committed_map(industry);

    commands.entity(section).with_children(|content| {
        // Treasury & arms header.
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(16.0),
                margin: UiRect::bottom(Val::Px(6.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("Industry"),
                    theme.font_bold(17.0),
                    TextColor(theme::GOLD),
                ));
                if let Some(buildable) = buildable {
                    row.spawn((
                        Text::new(format!("Treasury ${}", fmt_thousands(buildable.treasury))),
                        theme.font_bold(14.0),
                        TextColor(theme::GOLD),
                    ));
                    icon_label(
                        row,
                        &theme,
                        icons,
                        "commodities",
                        "Arms",
                        &format!("Arms: {}", buildable.arms),
                        13.0,
                        theme::GOLD,
                    );
                }
                if observer {
                    row.spawn((
                        Text::new("(observer — read only)"),
                        theme.font_italic(12.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                }
            });

        // Three columns.
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(20.0),
                width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|columns| {
                columns
                    .spawn(column_node())
                    .with_children(|col| column_production(col, &theme, icons, industry, observer));
                columns.spawn(column_node()).with_children(|col| {
                    column_buildings_warehouse(
                        col, &theme, icons, industry, &committed, &ui, observer,
                    )
                });
                columns.spawn(column_node()).with_children(|col| {
                    column_logistics_builds(col, &theme, icons, industry, buildable, observer)
                });
            });
    });
}

fn column_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        flex_basis: Val::Px(0.0),
        min_width: Val::Px(0.0),
        ..default()
    }
}

// ── Column 1: production chains + labor + education + immigration ───────

fn column_production(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    observer: bool,
) {
    let pf = &industry.production_forecast;
    section_title(col, theme, "Production Chains");

    // Timber.
    chain_heading(col, theme, "Timber");
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Timber → Lumber",
        "Lumber",
        pf.timber_chain.mill_cap,
        pf.timber_chain.mill_max_output,
        target(industry, "timber_mill"),
        pf.timber_chain.mill_output,
        &[(pf.timber_chain.mill_committed_timber, "Timber")],
        pf.timber_chain.mill_labor,
        IndustryAction::Chain {
            chain: "timber",
            step: "mill",
        },
    );
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Lumber → Furniture",
        "Furniture",
        pf.timber_chain.factory_cap,
        pf.timber_chain.factory_max_output,
        target(industry, "lumber_factory"),
        pf.timber_chain.factory_output,
        &[(pf.timber_chain.factory_committed_lumber, "Lumber")],
        pf.timber_chain.factory_labor,
        IndustryAction::Chain {
            chain: "timber",
            step: "factory",
        },
    );
    if pf.paper_chain.factory_cap > 0 {
        chain_card(
            col,
            theme,
            icons,
            observer,
            "Lumber → Paper",
            "Paper",
            pf.paper_chain.factory_cap,
            pf.paper_chain.factory_max_output,
            target(industry, "paper_factory"),
            pf.paper_chain.factory_output,
            &[(pf.paper_chain.factory_committed_lumber, "Lumber")],
            pf.paper_chain.factory_labor,
            IndustryAction::Chain {
                chain: "timber",
                step: "paper",
            },
        );
    }

    // Metal.
    chain_heading(col, theme, "Metal");
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Coal+Iron → Steel",
        "Steel",
        pf.metal_chain.mill_cap,
        pf.metal_chain.mill_max_output,
        target(industry, "metal_mill"),
        pf.metal_chain.mill_output,
        &[
            (pf.metal_chain.mill_committed_coal, "Coal"),
            (pf.metal_chain.mill_committed_iron, "Iron"),
        ],
        pf.metal_chain.mill_labor,
        IndustryAction::Chain {
            chain: "metal",
            step: "mill",
        },
    );
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Steel → Hardware",
        "Hardware",
        pf.metal_chain.factory_cap,
        pf.metal_chain.factory_max_output,
        target(industry, "steel_factory"),
        pf.metal_chain.factory_output,
        &[(pf.metal_chain.factory_committed_steel, "Steel")],
        pf.metal_chain.factory_labor,
        IndustryAction::Chain {
            chain: "metal",
            step: "factory",
        },
    );
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Steel → Arms",
        "Arms",
        pf.arms_chain.armory_cap,
        pf.arms_chain.armory_max_output,
        target(industry, "armory"),
        pf.arms_chain.armory_output,
        &[(pf.arms_chain.armory_committed_steel, "Steel")],
        pf.arms_chain.armory_labor,
        IndustryAction::Chain {
            chain: "arms",
            step: "armory",
        },
    );

    // Textile.
    chain_heading(col, theme, "Textile");
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Cotton/Wool → Fabric",
        "Fabric",
        pf.textile_chain.mill_cap,
        pf.textile_chain.mill_max_output,
        target(industry, "textile_mill"),
        pf.textile_chain.mill_output,
        &[
            (pf.textile_chain.mill_committed_cotton, "Cotton"),
            (pf.textile_chain.mill_committed_wool, "Wool"),
        ],
        pf.textile_chain.mill_labor,
        IndustryAction::Chain {
            chain: "textile",
            step: "mill",
        },
    );
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Fabric → Clothing",
        "Clothing",
        pf.textile_chain.factory_cap,
        pf.textile_chain.factory_max_output,
        target(industry, "garment_factory"),
        pf.textile_chain.factory_output,
        &[(pf.textile_chain.factory_committed_fabric, "Fabric")],
        pf.textile_chain.factory_labor,
        IndustryAction::Chain {
            chain: "textile",
            step: "factory",
        },
    );

    // Food.
    chain_heading(col, theme, "Food");
    chain_card(
        col,
        theme,
        icons,
        observer,
        "Grain+Fruit+Meat → Canned",
        "CannedFood",
        pf.food_chain.factory_cap,
        pf.food_chain.factory_max_output,
        target(industry, "canned_food_factory"),
        pf.food_chain.factory_output,
        &[
            (pf.food_chain.factory_committed_grain, "Grain"),
            (pf.food_chain.factory_committed_fruit, "Fruit"),
            (pf.food_chain.factory_committed_fish, "Fish"),
            (pf.food_chain.factory_committed_livestock, "Livestock"),
        ],
        pf.food_chain.factory_labor,
        IndustryAction::Chain {
            chain: "food",
            step: "factory",
        },
    );

    // Labor.
    section_title(col, theme, "Labor");
    let labor = &industry.labor;
    let production_labor = pf.timber_chain.mill_labor
        + pf.timber_chain.factory_labor
        + pf.metal_chain.mill_labor
        + pf.metal_chain.factory_labor
        + pf.textile_chain.mill_labor
        + pf.textile_chain.factory_labor
        + pf.arms_chain.armory_labor
        + pf.paper_chain.factory_labor
        + pf.food_chain.factory_labor;
    let committed_units = labor.committed_labor_units + production_labor;
    let free_units = labor.total_labor_units.saturating_sub(committed_units);
    labor_row(
        col,
        theme,
        "Untrained",
        labor.untrained,
        labor.committed_untrained,
        theme::TEXT_DIM,
    );
    labor_row(
        col,
        theme,
        "Trained",
        labor.trained,
        labor.committed_trained,
        COMMITTED_BLUE,
    );
    labor_row(
        col,
        theme,
        "Expert",
        labor.expert,
        labor.committed_expert,
        Color::srgb_u8(0x4a, 0x8f, 0xd4),
    );
    let summary = if committed_units > 0 {
        format!(
            "= {free_units} free of {} labor units",
            labor.total_labor_units
        )
    } else {
        format!("= {} labor units", labor.total_labor_units)
    };
    col.spawn((
        Text::new(summary),
        theme.font(11.0),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::top(Val::Px(2.0)),
            ..default()
        },
    ));

    // Education.
    section_title(col, theme, "Education");
    let costs = &industry.training_costs;
    let paper = industry
        .warehouse
        .materials
        .get("Paper")
        .copied()
        .unwrap_or(0);
    let max_by_paper = if costs.to_trained_paper > 0 {
        paper / costs.to_trained_paper
    } else {
        labor.untrained
    };
    let max_to_trained = labor.untrained.min(max_by_paper);
    let pending = industry.pending_training;
    education_row(
        col,
        theme,
        observer,
        "Untrained → Trained",
        &format!(
            "cost: {} Paper + {} labor each",
            costs.to_trained_paper, costs.to_trained_labor
        ),
        max_to_trained,
        pending.to_trained,
        IndustryAction::TrainToTrained,
    );
    if pending.to_trained > 0 && paper < pending.to_trained * costs.to_trained_paper {
        col.spawn((
            Text::new("Need more paper — queue Lumber → Paper output above"),
            theme.font(10.0),
            TextColor(theme::WARN),
        ));
    }
    education_row(
        col,
        theme,
        observer,
        "Trained → Expert",
        &format!(
            "cost: {} Paper + {} labor each",
            costs.to_expert_paper, costs.to_expert_labor
        ),
        labor.trained,
        pending.to_expert,
        IndustryAction::TrainToExpert,
    );

    // Immigration.
    section_title(col, theme, "Immigration");
    let costs = &industry.immigration_costs;
    sub_label_row(
        col,
        theme,
        "New Untrained Workers",
        &format!(
            "{} CannedFood + {} Clothing each",
            costs.canned_food, costs.clothing
        ),
    );
    let show = industry.max_pending_immigration > 0 || industry.pending_immigration > 0;
    if show {
        let max = industry
            .max_pending_immigration
            .max(industry.pending_immigration);
        spawn_action_slider(
            col,
            theme,
            max,
            industry.pending_immigration,
            IndustryAction::Immigration,
            observer,
        );
    } else {
        col.spawn((
            Text::new("Needs canned food (Food chain), clothing (Textile chain), and open slots"),
            theme.font(10.0),
            TextColor(theme::MUTED),
        ));
    }
}

fn target(industry: &IndustryVm, key: &str) -> u32 {
    industry
        .chain_targets
        .get(key)
        .copied()
        .unwrap_or(UNLIMITED)
}

fn chain_heading(col: &mut ChildSpawnerCommands, theme: &Theme, label: &str) {
    col.spawn((
        Text::new(label.to_string()),
        theme.font_bold(13.0),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::vertical(Val::Px(4.0)),
            ..default()
        },
    ));
}

/// One production step as an inset card (CC-1): icon arrow-diagram + name
/// on the first line, building status on the second, labeled "Output"
/// target slider (with the ∞ notch = unlimited) on the third. Steps without
/// a building render one muted card with the unblock hint (CC-3).
#[allow(clippy::too_many_arguments)]
fn chain_card(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    observer: bool,
    label: &str,
    output_icon: &str,
    cap: u32,
    max_output: u32,
    target: u32,
    output: u32,
    inputs: &[(u32, &str)],
    labor: u32,
    action: IndustryAction,
) {
    let built = cap > 0;

    let mut parts: Vec<String> = inputs
        .iter()
        .filter(|(qty, _)| *qty > 0)
        .map(|(qty, name)| format!("{qty} {name}"))
        .collect();
    if labor > 0 {
        parts.push(format!("{labor} labor"));
    }
    let summary = if parts.is_empty() {
        format!("→ {output}")
    } else {
        format!("{} → {output}", parts.join(" + "))
    };

    col.spawn(inset_panel()).with_children(|card| {
        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|line| {
            line.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(3.0),
                ..default()
            })
            .with_children(|diagram| {
                for (_, name) in inputs {
                    spawn_icon(diagram, icons, "commodities", name, 13.0);
                }
                diagram.spawn((Text::new("→"), theme.font(11.0), TextColor(theme::TEXT_DIM)));
                spawn_icon(diagram, icons, "commodities", output_icon, 13.0);
                diagram.spawn((
                    Text::new(format!("  {label}")),
                    theme.font(11.5),
                    TextColor(if built { theme::TEXT } else { theme::TEXT_DIM }),
                ));
            });
            if built {
                line.spawn((
                    Text::new(summary),
                    theme.font(11.0),
                    TextColor(if output > 0 {
                        theme::GOLD
                    } else {
                        theme::TEXT_DIM
                    }),
                ));
            }
        });

        if !built {
            // Honest dead-end: player nations currently have no way to
            // construct missing mills/factories (starting buildings depend
            // on difficulty), so don't send players hunting for one.
            card.spawn((
                Text::new("No building — this industry hasn't been developed yet"),
                theme.font_italic(10.0),
                TextColor(theme::TEXT_DIM),
            ));
            return;
        }

        let effective_cap = cap.min(max_output);
        let status = if max_output < cap {
            format!("Capacity {cap} · inputs limit output to {max_output}")
        } else {
            format!("Capacity {cap}")
        };
        card.spawn((
            Text::new(status),
            theme.font(10.0),
            TextColor(theme::TEXT_DIM),
        ));

        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|slider_row| {
            slider_row.spawn((
                Text::new("Output"),
                theme.font(10.5),
                TextColor(theme::TEXT_DIM),
            ));
            let value = if target == UNLIMITED {
                effective_cap as f32 + 1.0
            } else {
                target.min(effective_cap) as f32
            };
            let cap_for_label = effective_cap;
            let slider = widgets::spawn_slider(
                slider_row,
                theme,
                SliderProps {
                    min: 0.0,
                    max: effective_cap as f32,
                    step: 1.0,
                    value,
                    unlimited: true,
                    width: Val::Px(130.0),
                    format: Some(Arc::new(move |v| format!("{v:.0}/{cap_for_label}"))),
                },
            );
            let mut entity = slider_row.commands_mut().entity(slider);
            entity.insert((
                action,
                TooltipText(
                    "Caps this step's output per turn. The far-right ∞ notch means \
                     unlimited — produce as inputs allow."
                        .into(),
                ),
            ));
            if observer {
                entity.insert(InteractionDisabled);
            }
        });
    });
}

fn labor_row(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    label: &str,
    count: u32,
    committed: u32,
    color: Color,
) {
    let free = count.saturating_sub(committed);
    col.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(8.0),
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Text::new(label.to_string()),
            theme.font(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        let value = if committed > 0 {
            format!("{free} ({count})")
        } else {
            format!("{count}")
        };
        row.spawn((
            Text::new(value),
            theme.font_bold(12.0),
            TextColor(if committed > 0 { COMMITTED_BLUE } else { color }),
        ));
    });
}

fn sub_label_row(col: &mut ChildSpawnerCommands, theme: &Theme, left: &str, right: &str) {
    col.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(6.0),
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Text::new(left.to_string()),
            theme.font(11.0),
            TextColor(theme::TEXT),
        ));
        row.spawn((
            Text::new(right.to_string()),
            theme.font(10.0),
            TextColor(theme::TEXT_DIM),
        ));
    });
}

fn education_row(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    observer: bool,
    label: &str,
    cost: &str,
    max: u32,
    pending: u32,
    action: IndustryAction,
) {
    sub_label_row(col, theme, label, cost);
    spawn_action_slider(col, theme, max, pending, action, observer);
}

fn spawn_action_slider(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    max: u32,
    value: u32,
    action: IndustryAction,
    observer: bool,
) {
    let slider = widgets::spawn_slider(
        col,
        theme,
        SliderProps {
            min: 0.0,
            max: max as f32,
            step: 1.0,
            value: value as f32,
            width: Val::Px(150.0),
            ..default()
        },
    );
    let mut entity = col.commands_mut().entity(slider);
    entity.insert(action);
    if observer {
        entity.insert(InteractionDisabled);
    }
}

// ── Column 2: buildings + warehouse ──────────────────────────────────────

fn column_buildings_warehouse(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    ui: &IndustryUi,
    observer: bool,
) {
    section_title(col, theme, "Buildings");
    if industry.buildings.is_empty() {
        col.spawn((
            Text::new("No production buildings"),
            theme.font_italic(11.0),
            TextColor(theme::TEXT_DIM),
        ));
    }
    for building in &industry.buildings {
        let expandable = industry
            .can_expand
            .get(&building.building_type)
            .copied()
            .unwrap_or(false);
        col.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            margin: UiRect::bottom(Val::Px(2.0)),
            ..default()
        })
        .with_children(|row| {
            let capacity = if building.is_expanding {
                format!(
                    "{}→{}",
                    building.capacity,
                    building.capacity + building.pending_capacity
                )
            } else {
                format!("{}/{}", building.capacity, building.next_capacity)
            };
            row.spawn((
                Text::new(format!("{}  {capacity}", building.display_name)),
                theme.font(12.0),
                TextColor(theme::TEXT),
            ));
            if building.is_expanding {
                row.spawn((
                    Text::new(format!("{}t left", building.turns_remaining)),
                    theme.font(11.0),
                    TextColor(theme::GOLD),
                ));
            } else {
                let button = widgets::spawn_button(
                    row,
                    theme,
                    ButtonProps {
                        label: "Expand".into(),
                        font_size: 11.0,
                        enabled: expandable && !observer,
                        ..default()
                    },
                );
                row.commands().entity(button).insert((
                    ExpandButton(building.building_type.clone()),
                    TooltipText(format!(
                        "Cost: {} Lumber + {} Steel",
                        building.expansion_cost.lumber, building.expansion_cost.steel
                    )),
                ));
            }
        });
    }

    section_title(col, theme, "Warehouse");
    let checkbox = widgets::spawn_checkbox(
        col,
        theme,
        CheckboxProps {
            label: "Debug: show AI targets".into(),
            checked: ui.show_targets,
            enabled: true,
        },
    );
    col.commands_mut().entity(checkbox).insert((
        ShowTargetsCheckbox,
        TooltipText(
            "Show the AI's per-commodity stock target beside each entry. Resources use the \
             buy-side stockpile target; materials/goods use the sell-side reserve."
                .into(),
        ),
    ));

    col.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::FlexStart,
        column_gap: Val::Px(14.0),
        width: Val::Percent(100.0),
        margin: UiRect::top(Val::Px(4.0)),
        ..default()
    })
    .with_children(|grid| {
        grid.spawn(column_node()).with_children(|half| {
            warehouse_group(
                half,
                theme,
                icons,
                "Resources",
                &industry.warehouse.resources,
                &industry.warehouse_targets.resources,
                committed,
                ui.show_targets,
            );
        });
        grid.spawn(column_node()).with_children(|half| {
            warehouse_group(
                half,
                theme,
                icons,
                "Materials",
                &industry.warehouse.materials,
                &industry.warehouse_targets.materials,
                committed,
                ui.show_targets,
            );
        });
    });
    let goods_visible = industry.warehouse.goods.values().any(|v| *v > 0)
        || (ui.show_targets && industry.warehouse_targets.goods.values().any(|v| *v > 0));
    if goods_visible {
        warehouse_group(
            col,
            theme,
            icons,
            "Goods",
            &industry.warehouse.goods,
            &industry.warehouse_targets.goods,
            committed,
            ui.show_targets,
        );
    }
}

fn warehouse_group(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    title: &str,
    stock: &std::collections::BTreeMap<String, u32>,
    targets: &std::collections::BTreeMap<String, u32>,
    committed: &HashMap<String, u32>,
    show_targets: bool,
) {
    let mut keys: Vec<&String> = stock
        .iter()
        .filter(|(_, v)| **v > 0)
        .map(|(k, _)| k)
        .collect();
    if show_targets {
        for (k, v) in targets {
            if *v > 0 && !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys.sort();
    // CC-1: each warehouse section sits in its own inset container, with
    // counts right-aligned.
    parent.spawn(inset_panel()).with_children(|card| {
        card.spawn((
            Text::new(title.to_uppercase()),
            theme.font(9.5),
            TextColor(theme::TEXT_DIM),
            Node {
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
        ));
        if keys.is_empty() {
            card.spawn((
                Text::new("Empty"),
                theme.font_italic(10.5),
                TextColor(theme::TEXT_DIM),
            ));
        }
        for key in keys {
            let total = stock.get(key).copied().unwrap_or(0);
            let used = committed.get(key).copied().unwrap_or(0);
            let free = total.saturating_sub(used);
            let target = targets.get(key).copied().unwrap_or(0);
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                margin: UiRect::bottom(Val::Px(1.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|left| {
                    spawn_icon(left, icons, "commodities", key, 13.0);
                    left.spawn((
                        Text::new(split_camel(key)),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                    ));
                });
                let counts = row
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|right| {
                        if show_targets && target > 0 {
                            right.spawn((
                                Text::new(format!("aim {target}")),
                                theme.font(10.0),
                                TextColor(if total < target {
                                    TARGET_BEHIND
                                } else {
                                    TARGET_MET
                                }),
                            ));
                        }
                        if used > 0 {
                            right.spawn((
                                Text::new(format!("({total})")),
                                theme.font(10.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                        }
                        right.spawn((
                            Text::new(format!("{free}")),
                            theme.font_bold(12.0),
                            TextColor(if free < total {
                                COMMITTED_BLUE
                            } else {
                                theme::GOLD
                            }),
                        ));
                    })
                    .id();
                if used > 0 {
                    row.commands().entity(counts).insert(TooltipText(format!(
                        "{free} free of {total} in stock — {used} committed to queued orders"
                    )));
                }
            });
        }
    });
}

/// Warehouse amounts committed by queued production / training /
/// immigration / freight / recruits — the web panel's `committed` map.
fn committed_map(industry: &IndustryVm) -> HashMap<String, u32> {
    let pf = &industry.production_forecast;
    let mut map: HashMap<String, u32> = HashMap::new();
    let mut add = |key: &str, value: u32| {
        if value > 0 {
            *map.entry(key.to_string()).or_insert(0) += value;
        }
    };
    add("Timber", pf.timber_chain.mill_committed_timber);
    add("Coal", pf.metal_chain.mill_committed_coal);
    add("Iron", pf.metal_chain.mill_committed_iron);
    add("Cotton", pf.textile_chain.mill_committed_cotton);
    add("Wool", pf.textile_chain.mill_committed_wool);
    add("Lumber", pf.timber_chain.factory_committed_lumber);
    add("Steel", pf.metal_chain.factory_committed_steel);
    add("Fabric", pf.textile_chain.factory_committed_fabric);
    add("Steel", pf.arms_chain.armory_committed_steel);
    add("Lumber", pf.paper_chain.factory_committed_lumber);
    add("Grain", pf.food_chain.factory_committed_grain);
    add("Fruit", pf.food_chain.factory_committed_fruit);
    add("Fish", pf.food_chain.factory_committed_fish);
    add("Livestock", pf.food_chain.factory_committed_livestock);
    let training = &industry.pending_training;
    let costs = &industry.training_costs;
    add(
        "Paper",
        training.to_trained * costs.to_trained_paper + training.to_expert * costs.to_expert_paper,
    );
    if industry.pending_immigration > 0 {
        add(
            "CannedFood",
            industry.pending_immigration * industry.immigration_costs.canned_food,
        );
        add(
            "Clothing",
            industry.pending_immigration * industry.immigration_costs.clothing,
        );
    }
    // Freight cars: 1 lumber + 1 steel each.
    add("Lumber", industry.pending_freight_cars);
    add("Steel", industry.pending_freight_cars);
    add("Arms", industry.army_committed_arms);
    add("Horses", industry.army_committed_horses);
    map
}

// ── Column 3: logistics + army + naval + civilians ───────────────────────

fn column_logistics_builds(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    buildable: Option<&crate::game::vm::BuildableUnitsVm>,
    observer: bool,
) {
    section_title(col, theme, "Logistics");
    if industry.max_freight_cars == 0 && industry.pending_freight_cars == 0 {
        col.spawn((
            Text::new("Cannot build freight cars (need lumber + steel + labor)"),
            theme.font_italic(10.0),
            TextColor(theme::TEXT_DIM),
        ));
    } else {
        sub_label_row(
            col,
            theme,
            "Freight Cars",
            "1 Lumber + 1 Steel + 2 labor each",
        );
        spawn_action_slider(
            col,
            theme,
            industry.max_freight_cars.max(industry.pending_freight_cars),
            industry.pending_freight_cars,
            IndustryAction::FreightCars,
            observer,
        );
    }

    let Some(buildable) = buildable else {
        return;
    };

    // Army recruitment. While nothing is recruitable the nine "Not enough
    // arms" rows collapse into one muted group with a single unblock hint
    // (CC-3); it expands to full rows as soon as any unit can be queued.
    let army: Vec<&BuildableEntryVm> = buildable.army.iter().filter(|b| b.tech_met).collect();
    if !army.is_empty() {
        section_title(col, theme, "Army Recruitment");
        let queued_any = army
            .iter()
            .any(|entry| industry.pending_army_recruits.contains(&entry.unit_type));
        let locked = army.iter().all(|entry| entry.max_count == 0) && !queued_any;
        if locked {
            col.spawn(inset_panel()).with_children(|card| {
                for entry in &army {
                    let mut sublabel = format!("${}", entry.cost.unwrap_or(0));
                    if let Some(arms) = entry.arms_required.filter(|a| *a > 0) {
                        sublabel.push_str(&format!(" + {arms} Arms"));
                    }
                    card.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.0),
                            ..default()
                        })
                        .with_children(|left| {
                            let icon = unit_icon_name(entry.category.as_deref().unwrap_or(""));
                            spawn_icon(left, icons, "units", icon, 14.0);
                            left.spawn((
                                Text::new(split_camel(&entry.unit_type)),
                                theme.font(11.5),
                                TextColor(theme::TEXT_DIM),
                            ));
                        });
                        row.spawn((
                            Text::new(sublabel),
                            theme.font(10.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                    });
                }
                let hint = if buildable.arms == 0 {
                    "Not enough arms — set the Steel → Arms output slider under \
                     Production Chains, or buy Arms on the Trade screen (F5)"
                } else {
                    "Cannot recruit right now — check your treasury and arms stock"
                };
                card.spawn((
                    Text::new(hint),
                    theme.font_italic(10.5),
                    TextColor(theme::WARN),
                    Node {
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    },
                ));
            });
        } else {
            for entry in army {
                let queued = industry
                    .pending_army_recruits
                    .iter()
                    .filter(|s| **s == entry.unit_type)
                    .count() as u32;
                let mut sublabel = format!("${}", entry.cost.unwrap_or(0));
                if let Some(arms) = entry.arms_required.filter(|a| *a > 0) {
                    sublabel.push_str(&format!(" +{arms}A"));
                }
                let icon = unit_icon_name(entry.category.as_deref().unwrap_or(""));
                build_row(
                    col,
                    theme,
                    icons,
                    "units",
                    icon,
                    &split_camel(&entry.unit_type),
                    &sublabel,
                    None,
                    entry.max_count,
                    queued,
                    entry.tech_met,
                    if entry.max_count == 0 {
                        Some(reason_with_hint(
                            entry.reason.clone().unwrap_or("Cannot recruit".into()),
                        ))
                    } else {
                        None
                    },
                    IndustryAction::Recruit(entry.unit_type.clone()),
                    observer,
                );
            }
        }
    }

    // Naval construction.
    let ships: Vec<&BuildableEntryVm> = buildable.ships.iter().filter(|b| b.tech_met).collect();
    if !ships.is_empty() {
        section_title(col, theme, "Naval Construction");
        for category in ["Warship", "Merchant"] {
            let in_category: Vec<&&BuildableEntryVm> = ships
                .iter()
                .filter(|b| b.category.as_deref() == Some(category))
                .collect();
            if in_category.is_empty() {
                continue;
            }
            col.spawn((
                Text::new(format!("{}S", category.to_uppercase())),
                theme.font(9.5),
                TextColor(theme::TEXT_DIM),
                Node {
                    margin: UiRect::vertical(Val::Px(2.0)),
                    ..default()
                },
            ));
            for entry in in_category {
                let queued = industry
                    .pending_ships
                    .iter()
                    .filter(|s| **s == entry.unit_type)
                    .count() as u32;
                let sublabel = entry
                    .resources_needed
                    .as_ref()
                    .map(|needs| {
                        needs
                            .iter()
                            .map(|(k, v)| {
                                format!("{v}{}", k.chars().next().unwrap_or('?').to_uppercase())
                            })
                            .collect::<Vec<_>>()
                            .join("+")
                    })
                    .unwrap_or_default();
                let reason = if !entry.tech_met {
                    Some(entry.reason.clone().unwrap_or("Tech required".into()))
                } else if entry.max_count == 0 {
                    Some(reason_with_hint("Insufficient resources".to_string()))
                } else {
                    None
                };
                let cost_tooltip = entry.resources_needed.as_ref().map(|needs| {
                    format!(
                        "Needs: {}",
                        needs
                            .iter()
                            .map(|(k, v)| format!("{v} {}", split_camel(k)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                });
                build_row(
                    col,
                    theme,
                    icons,
                    "ships",
                    &entry.unit_type,
                    &split_camel(&entry.unit_type),
                    &sublabel,
                    cost_tooltip,
                    entry.max_count,
                    queued,
                    entry.tech_met,
                    reason,
                    IndustryAction::Ship(entry.unit_type.clone()),
                    observer,
                );
            }
        }
    }

    // Civilian hiring.
    let civilians: Vec<&BuildableEntryVm> =
        buildable.civilians.iter().filter(|b| b.tech_met).collect();
    if !civilians.is_empty() {
        section_title(col, theme, "Civilian Hiring");
        for entry in civilians {
            let pending = industry
                .pending_civilian_hires
                .get(&entry.unit_type)
                .copied()
                .unwrap_or(0);
            let expert = entry.expert_required.unwrap_or(false);
            let sublabel = if expert {
                format!("${} + 1 expert", entry.cost.unwrap_or(0))
            } else {
                format!("${}", entry.cost.unwrap_or(0))
            };
            let reason = if entry.max_count == 0 {
                Some(reason_with_hint(
                    if expert {
                        "Need expert workers"
                    } else {
                        "Cannot afford"
                    }
                    .to_string(),
                ))
            } else {
                None
            };
            build_row(
                col,
                theme,
                icons,
                "civilians",
                &entry.unit_type,
                &entry.unit_type,
                &sublabel,
                None,
                entry.max_count,
                pending,
                entry.tech_met,
                reason,
                IndustryAction::Hire(entry.unit_type.clone()),
                observer,
            );
        }
    }
}

/// CC-3: append the unblock hint for a known disabled reason.
fn reason_with_hint(reason: String) -> String {
    let lower = reason.to_lowercase();
    if lower.contains("arms") {
        format!("{reason} — set the Steel → Arms output slider under Production Chains")
    } else if lower.contains("horses") {
        format!("{reason} — buy Horses on the Trade screen (F5)")
    } else if lower.contains("expert") {
        format!("{reason} — train experts under Education")
    } else if lower.contains("afford") {
        format!("{reason} — treasury too low this turn")
    } else if lower.contains("resources") {
        format!("{reason} — produce the needed materials in your industry")
    } else {
        reason
    }
}

/// Recruit/ship/hire row: icon + name + cost sublabel, with either a
/// queue slider or a greyed disabled-reason line (web `ShipBuildRow`).
#[allow(clippy::too_many_arguments)]
fn build_row(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    icon_group: &str,
    icon_name: &str,
    label: &str,
    sublabel: &str,
    sublabel_tooltip: Option<String>,
    max_count: u32,
    queued: u32,
    tech_met: bool,
    reason: Option<String>,
    action: IndustryAction,
    observer: bool,
) {
    let can_build = tech_met && max_count > 0;
    let show_slider = can_build || queued > 0;
    col.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::FlexStart,
        column_gap: Val::Px(6.0),
        margin: UiRect::bottom(Val::Px(3.0)),
        ..default()
    })
    .with_children(|row| {
        spawn_icon(row, icons, icon_group, icon_name, 16.0);
        row.spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            ..default()
        })
        .with_children(|body| {
            body.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn((
                    Text::new(label.to_string()),
                    theme.font(12.0),
                    TextColor(if show_slider {
                        theme::TEXT
                    } else {
                        theme::TEXT_DIM
                    }),
                ));
                let mut sub = line.spawn((
                    Text::new(sublabel.to_string()),
                    theme.font(10.0),
                    TextColor(theme::TEXT_DIM),
                ));
                if let Some(tooltip) = sublabel_tooltip {
                    sub.insert(TooltipText(tooltip));
                }
            });
            if show_slider {
                let max = max_count.max(queued);
                let slider = widgets::spawn_slider(
                    body,
                    theme,
                    SliderProps {
                        min: 0.0,
                        max: max as f32,
                        step: 1.0,
                        value: queued as f32,
                        width: Val::Px(130.0),
                        format: Some(Arc::new(|v| {
                            if v > 0.0 {
                                format!("+{v:.0}")
                            } else {
                                "0".to_string()
                            }
                        })),
                        ..default()
                    },
                );
                let mut entity = body.commands_mut().entity(slider);
                entity.insert(action.clone());
                if observer {
                    entity.insert(InteractionDisabled);
                }
            } else if let Some(reason) = reason {
                body.spawn((Text::new(reason), theme.font(10.0), TextColor(theme::MUTED)));
            }
        });
    });
}

// ── Interaction ──────────────────────────────────────────────────────────

pub fn handle_industry_sliders(
    mut commits: MessageReader<SliderCommitted>,
    actions: Query<&IndustryAction>,
    vms: Res<ViewModels>,
    mut out: MessageWriter<GameCommand>,
) {
    for commit in commits.read() {
        let Ok(action) = actions.get(commit.entity) else {
            continue;
        };
        let value = commit.as_u32();
        match action {
            IndustryAction::Chain { chain, step } => {
                out.write(GameCommand::SetChainTarget {
                    chain,
                    step,
                    target: value,
                });
            }
            IndustryAction::TrainToTrained => {
                let to_expert = vms
                    .industry
                    .as_ref()
                    .map(|i| i.pending_training.to_expert)
                    .unwrap_or(0);
                out.write(GameCommand::SetPendingTraining {
                    to_trained: value,
                    to_expert,
                });
            }
            IndustryAction::TrainToExpert => {
                let to_trained = vms
                    .industry
                    .as_ref()
                    .map(|i| i.pending_training.to_trained)
                    .unwrap_or(0);
                out.write(GameCommand::SetPendingTraining {
                    to_trained,
                    to_expert: value,
                });
            }
            IndustryAction::Immigration => {
                out.write(GameCommand::SetPendingImmigration { count: value });
            }
            IndustryAction::FreightCars => {
                out.write(GameCommand::SetPendingFreightCars { count: value });
            }
            IndustryAction::Recruit(unit_type) => {
                out.write(GameCommand::SetPendingArmyRecruits {
                    unit_type: unit_type.clone(),
                    count: value,
                });
            }
            IndustryAction::Ship(ship_type) => {
                out.write(GameCommand::SetPendingShips {
                    ship_type: ship_type.clone(),
                    count: value,
                });
            }
            IndustryAction::Hire(civilian_type) => {
                out.write(GameCommand::SetPendingCivilianHire {
                    civilian_type: civilian_type.clone(),
                    count: value,
                });
            }
        }
    }
}

pub fn handle_industry_buttons(
    mut activations: MessageReader<ButtonActivated>,
    expand: Query<&ExpandButton>,
    mut out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(button) = expand.get(*entity) {
            out.write(GameCommand::ExpandBuilding {
                building_type: button.0.clone(),
            });
        }
    }
}

pub fn handle_show_targets(
    mut toggles: MessageReader<CheckboxToggled>,
    boxes: Query<(), With<ShowTargetsCheckbox>>,
    mut ui: ResMut<IndustryUi>,
) {
    for toggle in toggles.read() {
        if boxes.contains(toggle.entity) {
            ui.show_targets = toggle.checked;
        }
    }
}
