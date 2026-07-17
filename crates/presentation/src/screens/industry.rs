//! Industry screen (F3): full-screen card deck. One card per decision area
//! — Production (chain tiles with merged capacity/expand), Workforce
//! (labor / education / immigration / civilian hiring), Warehouse (stock
//! shelves), and Recruit (army / navy / logistics). Cards page with ←/→,
//! the 1–4 hotkeys, the edge chevrons, or the icon tab strip; a contextual
//! footer strip keeps the stocks relevant to the active card in view.
//! Every control merely queues pending state; the end-turn pipeline
//! applies it.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, ViewModels};
use crate::game::vm::{BuildableEntryVm, BuildableUnitsVm, BuildingVm, IndustryVm};
use crate::map::icons::IconAssets;
use crate::screens::common::{
    fmt_thousands, icon_label, inset_panel, section_title, spawn_icon, split_camel, unit_icon_name,
};
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, CheckboxProps, CheckboxToggled, ModalStack,
    SliderCommitted, SliderProps, TooltipText, UNLIMITED,
};

const COMMITTED_BLUE: Color = Color::srgb_u8(0x6a, 0xb0, 0xd4);
const TARGET_BEHIND: Color = Color::srgb_u8(0xd9, 0x7a, 0x4a);
const TARGET_MET: Color = Color::srgb_u8(0x66, 0xaa, 0x88);

// ── Markers / state ──────────────────────────────────────────────────────

#[derive(Component)]
pub struct IndustryRoot;

/// Pinned header: title, treasury, icon tab strip, close.
#[derive(Component)]
pub struct IndustryChrome;

/// Scrollable body of the active card.
#[derive(Component)]
pub struct IndustryContent;

/// Pinned contextual footer strip.
#[derive(Component)]
pub struct IndustryFooter;

/// The four full-screen cards, in tab-strip / hotkey order.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum IndustryCard {
    #[default]
    Production,
    Workforce,
    Warehouse,
    Recruit,
}

impl IndustryCard {
    pub const ALL: [IndustryCard; 4] = [
        IndustryCard::Production,
        IndustryCard::Workforce,
        IndustryCard::Warehouse,
        IndustryCard::Recruit,
    ];

    fn title(self) -> &'static str {
        match self {
            IndustryCard::Production => "Production",
            IndustryCard::Workforce => "Workforce",
            IndustryCard::Warehouse => "Warehouse",
            IndustryCard::Recruit => "Recruit",
        }
    }

    /// `(group, name)` of the tab-strip icon.
    fn icon(self) -> (&'static str, &'static str) {
        match self {
            IndustryCard::Production => ("ui", "Factory"),
            IndustryCard::Workforce => ("ui", "Workers"),
            IndustryCard::Warehouse => ("ui", "Warehouse"),
            IndustryCard::Recruit => ("ui", "Swords"),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    fn offset(self, delta: isize) -> IndustryCard {
        let len = Self::ALL.len() as isize;
        let index = (self.index() as isize + delta).rem_euclid(len);
        Self::ALL[index as usize]
    }
}

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

#[derive(Component)]
pub struct IndustryCloseButton;

/// Tab-strip button jumping straight to a card.
#[derive(Component)]
pub struct CardTabButton(pub IndustryCard);

/// Edge chevron paging one card back (-1) or forward (+1).
#[derive(Component)]
pub struct CardNavButton(pub isize);

/// Screen-local UI state: the AI-target debug toggle (web `showTargets`
/// parity) and the active card, which survives close/reopen.
#[derive(Resource, Default)]
pub struct IndustryUi {
    pub show_targets: bool,
    pub active_card: IndustryCard,
}

// ── Lifecycle ────────────────────────────────────────────────────────────

pub fn enter_industry(mut commands: Commands, theme: Res<Theme>) {
    let root = crate::screens::common::full_screen_root(&mut commands);
    commands.entity(root).insert(IndustryRoot);
    commands.entity(root).with_children(|panel| {
        panel.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            IndustryChrome,
        ));
        panel
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|middle| {
                nav_chevron(middle, &theme, "<", -1, "Previous card (Left arrow)");
                let scroll = widgets::spawn_scroll_area(
                    middle,
                    &theme,
                    widgets::ScrollProps {
                        width: Val::Auto,
                        flex_grow: 1.0,
                        ..default()
                    },
                );
                middle
                    .commands()
                    .entity(scroll.content)
                    .insert(IndustryContent);
                nav_chevron(middle, &theme, ">", 1, "Next card (Right arrow)");
            });
        panel.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(14.0),
                border: UiRect::top(Val::Px(1.0)),
                padding: UiRect::top(Val::Px(7.0)),
                margin: UiRect::top(Val::Px(7.0)),
                ..default()
            },
            BorderColor::all(theme::BORDER),
            IndustryFooter,
        ));
    });
}

/// Full-height edge chevron (a huge click target for mouse paging).
fn nav_chevron(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    glyph: &str,
    delta: isize,
    tooltip: &str,
) {
    let button = widgets::spawn_button(
        parent,
        theme,
        ButtonProps {
            label: glyph.into(),
            font_size: 20.0,
            ..default()
        },
    );
    parent.commands().entity(button).insert((
        CardNavButton(delta),
        Node {
            width: Val::Px(30.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            flex_shrink: 0.0,
            ..default()
        },
        TooltipText(tooltip.into()),
    ));
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
    chromes: Query<Entity, With<IndustryChrome>>,
    contents: Query<Entity, With<IndustryContent>>,
    footers: Query<Entity, With<IndustryFooter>>,
    added: Query<(), Added<IndustryContent>>,
) {
    if !vms.is_changed() && !ui.is_changed() && added.is_empty() {
        return;
    }
    let (Ok(chrome), Ok(content), Ok(footer)) =
        (chromes.single(), contents.single(), footers.single())
    else {
        return;
    };
    commands.entity(chrome).despawn_children();
    commands.entity(content).despawn_children();
    commands.entity(footer).despawn_children();
    let Some(industry) = vms.industry.as_ref() else {
        return;
    };
    let icons = icons.as_deref();
    let observer = meta.observer;
    let buildable = vms.buildable.as_ref();
    let committed = committed_map(industry, buildable);
    let card = ui.active_card;
    let attention = attention_cards(industry);

    commands.entity(chrome).with_children(|bar| {
        spawn_chrome(bar, &theme, icons, card, buildable, observer, &attention);
    });
    commands.entity(content).with_children(|body| match card {
        IndustryCard::Production => {
            card_production(body, &theme, icons, industry, &committed, observer)
        }
        IndustryCard::Workforce => {
            card_workforce(body, &theme, icons, industry, buildable, observer)
        }
        IndustryCard::Warehouse => card_warehouse(body, &theme, icons, industry, &committed, &ui),
        IndustryCard::Recruit => card_recruit(
            body, &theme, icons, industry, &committed, buildable, observer,
        ),
    });
    commands.entity(footer).with_children(|strip| {
        spawn_footer(strip, &theme, icons, industry, buildable, &committed, card);
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

// ── Chrome: header + icon tab strip ─────────────────────────────────────

/// Cards whose queued orders are currently under-resourced — their tabs get
/// a warning badge so the shortfall isn't missed while paging elsewhere.
/// Today only Workforce shortfalls are detected (training paper, immigration
/// food/clothing); Production and Recruit shortages surface through the
/// in-card red badges instead.
fn attention_cards(industry: &IndustryVm) -> Vec<IndustryCard> {
    let mut cards = Vec::new();
    let warehouse = &industry.warehouse;
    let stock = |key: &str| {
        warehouse
            .resources
            .get(key)
            .or_else(|| warehouse.materials.get(key))
            .or_else(|| warehouse.goods.get(key))
            .copied()
            .unwrap_or(0)
    };
    let pending = industry.pending_training;
    let costs = &industry.training_costs;
    let paper_needed =
        pending.to_trained * costs.to_trained_paper + pending.to_expert * costs.to_expert_paper;
    let mut workforce_short = paper_needed > stock("Paper");
    if industry.pending_immigration > 0 {
        let costs = &industry.immigration_costs;
        workforce_short |= industry.pending_immigration * costs.canned_food > stock("CannedFood")
            || industry.pending_immigration * costs.clothing > stock("Clothing");
    }
    if workforce_short {
        cards.push(IndustryCard::Workforce);
    }
    cards
}

#[allow(clippy::too_many_arguments)]
fn spawn_chrome(
    bar: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    active: IndustryCard,
    buildable: Option<&BuildableUnitsVm>,
    observer: bool,
    attention: &[IndustryCard],
) {
    bar.spawn(Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(14.0),
        margin: UiRect::bottom(Val::Px(6.0)),
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Text::new("Industry"),
            theme.font_bold(18.0),
            TextColor(theme::GOLD),
        ));
        if let Some(buildable) = buildable {
            icon_label(
                row,
                theme,
                icons,
                "ui",
                "Treasury",
                &format!("${}", fmt_thousands(buildable.treasury)),
                13.5,
                theme::GOLD,
            );
            icon_label(
                row,
                theme,
                icons,
                "commodities",
                "Arms",
                &format!("{}", buildable.arms),
                13.5,
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
        row.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        for card in IndustryCard::ALL {
            tab_button(
                row,
                theme,
                icons,
                card,
                card == active,
                attention.contains(&card),
            );
        }
        row.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        let close = widgets::spawn_button(
            row,
            theme,
            ButtonProps {
                label: "Close (Esc)".into(),
                font_size: 12.0,
                ..default()
            },
        );
        row.commands().entity(close).insert(IndustryCloseButton);
    });
}

/// Icon tab: pictogram + hotkey digit; the active tab grows a gold
/// underline and carries the card title (there is no separate heading
/// inside the card body). `attention` adds a warning badge.
fn tab_button(
    row: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    card: IndustryCard,
    active: bool,
    attention: bool,
) {
    let button = widgets::spawn_button(
        row,
        theme,
        ButtonProps {
            label: String::new(),
            flat: true,
            auto_label_tint: false,
            ..default()
        },
    );
    let (group, name) = card.icon();
    let mut commands = row.commands();
    let mut entity = commands.entity(button);
    entity.insert((
        CardTabButton(card),
        Node {
            height: Val::Px(34.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(5.0),
            padding: UiRect::axes(Val::Px(9.0), Val::Px(2.0)),
            border: UiRect::bottom(Val::Px(2.0)),
            flex_shrink: 0.0,
            ..default()
        },
        BorderColor::all(if active { theme::GOLD } else { Color::NONE }),
        TooltipText(format!(
            "{}{} — hotkey {} (Left/Right arrows page)",
            card.title(),
            if attention {
                " (queued orders are short on inputs)"
            } else {
                ""
            },
            card.index() + 1
        )),
    ));
    entity.with_children(|tab| {
        spawn_icon(tab, icons, group, name, 22.0);
        tab.spawn((
            Text::new(format!("{}", card.index() + 1)),
            theme.font(10.0),
            TextColor(if active { theme::GOLD } else { theme::TEXT_DIM }),
        ));
        if attention {
            tab.spawn((
                Text::new("!"),
                theme.font_bold(13.0),
                TextColor(theme::WARN),
            ));
        }
        if active {
            tab.spawn((
                Text::new(card.title()),
                theme.font_bold(13.5),
                TextColor(theme::GOLD),
            ));
        }
    });
}

// ── Contextual footer strip ──────────────────────────────────────────────

fn spawn_footer(
    strip: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    buildable: Option<&BuildableUnitsVm>,
    committed: &HashMap<String, u32>,
    card: IndustryCard,
) {
    let treasury = |strip: &mut ChildSpawnerCommands| {
        if let Some(buildable) = buildable {
            icon_label(
                strip,
                theme,
                icons,
                "ui",
                "Treasury",
                &format!("${}", fmt_thousands(buildable.treasury)),
                12.5,
                theme::GOLD,
            );
        }
    };
    match card {
        IndustryCard::Production => {
            labor_gauge(strip, theme, icons, industry);
            spacer(strip);
            for key in [
                "Timber",
                "Coal",
                "Iron",
                "Cotton",
                "Wool",
                "Lumber",
                "Steel",
                "Fabric",
                "Grain",
                "Fruit",
                "Fish",
                "Livestock",
            ] {
                stock_entry(strip, theme, icons, industry, committed, key);
            }
        }
        IndustryCard::Workforce => {
            labor_gauge(strip, theme, icons, industry);
            spacer(strip);
            treasury(strip);
            for key in ["Paper", "CannedFood", "Clothing"] {
                stock_entry(strip, theme, icons, industry, committed, key);
            }
        }
        IndustryCard::Warehouse => {
            treasury(strip);
            spacer(strip);
            strip.spawn((
                Text::new(
                    "Bold counts are free stock; (total) includes amounts committed to \
                     queued orders",
                ),
                theme.font_italic(11.0),
                TextColor(theme::TEXT_DIM),
            ));
        }
        IndustryCard::Recruit => {
            treasury(strip);
            spacer(strip);
            for key in ["Arms", "Horses", "Lumber", "Steel"] {
                stock_entry(strip, theme, icons, industry, committed, key);
            }
            labor_gauge(strip, theme, icons, industry);
        }
    }
}

fn spacer(parent: &mut ChildSpawnerCommands) {
    parent.spawn(Node {
        flex_grow: 1.0,
        ..default()
    });
}

/// Icon + free count for one commodity (tooltip carries name and totals).
fn stock_entry(
    strip: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    key: &str,
) {
    let warehouse = &industry.warehouse;
    let total = warehouse
        .resources
        .get(key)
        .or_else(|| warehouse.materials.get(key))
        .or_else(|| warehouse.goods.get(key))
        .copied()
        .unwrap_or(0);
    let used = committed.get(key).copied().unwrap_or(0);
    let free = total.saturating_sub(used);
    let color = if total == 0 {
        theme::TEXT_DIM
    } else if used > 0 {
        COMMITTED_BLUE
    } else {
        theme::GOLD
    };
    strip
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(3.0),
                ..default()
            },
            TooltipText(if used > 0 {
                format!(
                    "{}: {free} free of {total} — {used} committed to queued orders",
                    split_camel(key)
                )
            } else {
                format!("{}: {total} in stock", split_camel(key))
            }),
        ))
        .with_children(|entry| {
            spawn_icon(entry, icons, "commodities", key, 17.0);
            entry.spawn((
                Text::new(format!("{free}")),
                theme.font_bold(12.5),
                TextColor(color),
            ));
        });
}

/// Workers icon, free/total text, and a committed-share bar. The labor pool
/// is the budget every Production slider draws from, so it stays pinned.
fn labor_gauge(
    strip: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
) {
    let labor = &industry.labor;
    let committed = labor.committed_labor_units + production_labor(industry);
    let total = labor.total_labor_units;
    let free = total.saturating_sub(committed);
    let share = if total > 0 {
        (committed as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    strip
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
            TooltipText(format!(
                "Labor units: {committed} of {total} committed to production and queued \
                 orders, {free} free"
            )),
        ))
        .with_children(|gauge| {
            spawn_icon(gauge, icons, "ui", "Workers", 18.0);
            gauge.spawn((
                Text::new(format!("{free} free / {total}")),
                theme.font_bold(13.0),
                TextColor(if free == 0 && total > 0 {
                    theme::WARN
                } else {
                    theme::GOLD
                }),
            ));
            gauge
                .spawn((
                    Node {
                        width: Val::Px(100.0),
                        height: Val::Px(9.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(theme::INSET_BG),
                    BorderColor::all(theme::BORDER),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Node {
                            width: Val::Percent(share * 100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(COMMITTED_BLUE),
                    ));
                });
        });
}

/// Labor units consumed by the queued production-chain outputs.
fn production_labor(industry: &IndustryVm) -> u32 {
    let pf = &industry.production_forecast;
    pf.timber_chain.mill_labor
        + pf.timber_chain.factory_labor
        + pf.metal_chain.mill_labor
        + pf.metal_chain.factory_labor
        + pf.textile_chain.mill_labor
        + pf.textile_chain.factory_labor
        + pf.arms_chain.armory_labor
        + pf.paper_chain.factory_labor
        + pf.food_chain.factory_labor
}

// ── Card 1: Production ───────────────────────────────────────────────────

/// Chain steps whose building capacity is merged into their tile (the old
/// separate "Buildings" list). Anything else a nation owns (late-game
/// refineries, power plants, …) falls through to "Other Facilities".
const MERGED_BUILDINGS: [&str; 9] = [
    "LumberMill",
    "FurnitureFactory",
    "PaperFactory",
    "SteelMill",
    "HardwareFactory",
    "Armory",
    "TextileMill",
    "ClothingFactory",
    "FoodProcessing",
];

fn card_production(
    content: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    observer: bool,
) {
    let pf = &industry.production_forecast;
    content
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|columns| {
            columns.spawn(column_node()).with_children(|col| {
                family_heading(col, theme, "Timber");
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Timber"],
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
                    "LumberMill",
                );
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Lumber"],
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
                    "FurnitureFactory",
                );
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Lumber"],
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
                    "PaperFactory",
                );
            });
            columns.spawn(column_node()).with_children(|col| {
                family_heading(col, theme, "Metal");
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Coal", "Iron"],
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
                    "SteelMill",
                );
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Steel"],
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
                    "HardwareFactory",
                );
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Steel"],
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
                    "Armory",
                );
            });
            columns.spawn(column_node()).with_children(|col| {
                family_heading(col, theme, "Textile");
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Cotton", "Wool"],
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
                    "TextileMill",
                );
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Fabric"],
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
                    "ClothingFactory",
                );
                family_heading(col, theme, "Food");
                chain_tile(
                    col,
                    theme,
                    icons,
                    industry,
                    committed,
                    observer,
                    &["Grain", "Fruit", "Fish", "Livestock"],
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
                    "FoodProcessing",
                );
            });
        });

    // Late-game facilities with no chain tile keep their expand access.
    let others: Vec<&BuildingVm> = industry
        .buildings
        .iter()
        .filter(|b| !MERGED_BUILDINGS.contains(&b.building_type.as_str()))
        .collect();
    if !others.is_empty() {
        section_title(content, theme, "Other Facilities");
        for building in others {
            building_row(content, theme, industry, building, observer);
        }
    }
}

fn family_heading(col: &mut ChildSpawnerCommands, theme: &Theme, label: &str) {
    col.spawn((
        Text::new(label.to_uppercase()),
        theme.font_bold(12.5),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::vertical(Val::Px(4.0)),
            ..default()
        },
    ));
}

fn target(industry: &IndustryVm, key: &str) -> u32 {
    industry
        .chain_targets
        .get(key)
        .copied()
        .unwrap_or(UNLIMITED)
}

/// One production step as a full-width tile: icon recipe + capacity/expand
/// on the first line, the output slider (∞ notch = unlimited) as the hero
/// control, and an icon-math forecast of what the step actually does next
/// turn. Steps without a building collapse to a slim muted row.
#[allow(clippy::too_many_arguments)]
fn chain_tile(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    observer: bool,
    recipe: &[&str],
    output_icon: &str,
    cap: u32,
    max_output: u32,
    target: u32,
    output: u32,
    forecast: &[(u32, &str)],
    labor: u32,
    action: IndustryAction,
    building_key: &str,
) {
    let built = cap > 0;
    let building = industry
        .buildings
        .iter()
        .find(|b| b.building_type == building_key);
    let expandable = industry
        .can_expand
        .get(building_key)
        .copied()
        .unwrap_or(false);

    col.spawn(inset_panel()).with_children(|card| {
        // Recipe + capacity/expand line.
        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|line| {
            line.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|diagram| {
                for name in recipe {
                    // A drained input that caps this step's output gets the
                    // red shortage badge instead of a warning text line.
                    let short =
                        built && max_output < cap && free_stock(industry, committed, name) == 0;
                    let mut icon = diagram.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        ..default()
                    });
                    icon.insert(TooltipText(if short {
                        format!(
                            "{} — not enough in stock, limits output to {max_output}",
                            split_camel(name)
                        )
                    } else {
                        split_camel(name)
                    }));
                    icon.with_children(|slot| {
                        badged_icon(slot, theme, icons, "commodities", name, 20.0, short);
                    });
                }
                diagram.spawn((Text::new("→"), theme.font(14.0), TextColor(theme::TEXT_DIM)));
                let mut icon = diagram.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        ..default()
                    },
                    TooltipText(split_camel(output_icon)),
                ));
                icon.with_children(|slot| {
                    spawn_icon(slot, icons, "commodities", output_icon, 22.0);
                });
            });
            if built {
                line.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|right| {
                    right.spawn((
                        Text::new(format!("cap {cap}")),
                        theme.font(12.0),
                        TextColor(theme::TEXT_DIM),
                        TooltipText(
                            "Building capacity: the most this step can produce per turn".into(),
                        ),
                    ));
                    if let Some(building) = building {
                        if building.is_expanding {
                            right.spawn((
                                Text::new(format!(
                                    "→{} · {}t",
                                    building.capacity + building.pending_capacity,
                                    building.turns_remaining
                                )),
                                theme.font(11.5),
                                TextColor(theme::GOLD),
                                TooltipText(format!(
                                    "Expanding to capacity {} — {} turns remaining",
                                    building.capacity + building.pending_capacity,
                                    building.turns_remaining
                                )),
                            ));
                        } else {
                            let button = widgets::spawn_button(
                                right,
                                theme,
                                ButtonProps {
                                    label: "Expand".into(),
                                    font_size: 10.5,
                                    enabled: expandable && !observer,
                                    ..default()
                                },
                            );
                            right.commands().entity(button).insert((
                                ExpandButton(building.building_type.clone()),
                                TooltipText(format!(
                                    "Expand to capacity {}. Cost: {} Lumber + {} Steel",
                                    building.next_capacity,
                                    building.expansion_cost.lumber,
                                    building.expansion_cost.steel
                                )),
                            ));
                        }
                    }
                });
            }
        });

        if !built {
            // Honest dead-end: player nations currently have no way to
            // construct missing mills/factories (starting buildings depend
            // on difficulty), so don't send players hunting for one.
            card.spawn((
                Text::new("Not developed yet"),
                theme.font_italic(11.0),
                TextColor(theme::TEXT_DIM),
            ));
            return;
        }

        // Hero control: the output target slider.
        let effective_cap = cap.min(max_output);
        let value = if target == UNLIMITED {
            effective_cap as f32 + 1.0
        } else {
            target.min(effective_cap) as f32
        };
        let cap_for_label = effective_cap;
        let slider = widgets::spawn_slider(
            card,
            theme,
            SliderProps {
                min: 0.0,
                max: effective_cap as f32,
                step: 1.0,
                value,
                unlimited: true,
                width: Val::Px(150.0),
                format: Some(Arc::new(move |v| format!("{v:.0}/{cap_for_label}"))),
            },
        );
        let mut entity = card.commands_mut().entity(slider);
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

        // Icon-math forecast: inputs + labor → output, next turn.
        if output > 0 {
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|math| {
                let mut first = true;
                for (qty, name) in forecast.iter().filter(|(qty, _)| *qty > 0) {
                    if !first {
                        plus_sign(math, theme);
                    }
                    first = false;
                    qty_icon(
                        math,
                        theme,
                        icons,
                        "commodities",
                        name,
                        *qty,
                        theme::TEXT,
                        false,
                    );
                }
                if labor > 0 {
                    if !first {
                        plus_sign(math, theme);
                    }
                    qty_icon(
                        math,
                        theme,
                        icons,
                        "ui",
                        "Workers",
                        labor,
                        theme::TEXT,
                        false,
                    );
                }
                math.spawn((Text::new("→"), theme.font(13.0), TextColor(theme::TEXT_DIM)));
                qty_icon(
                    math,
                    theme,
                    icons,
                    "commodities",
                    output_icon,
                    output,
                    theme::GOLD,
                    false,
                );
            });
        } else {
            card.spawn((
                Text::new("No output next turn"),
                theme.font_italic(11.0),
                TextColor(theme::TEXT_DIM),
            ));
        }
    });
}

fn plus_sign(row: &mut ChildSpawnerCommands, theme: &Theme) {
    row.spawn((Text::new("+"), theme.font(12.0), TextColor(theme::TEXT_DIM)));
}

/// Icon with an optional small red "x" badge at the bottom-left flagging
/// insufficient stock — the icon-math replacement for "not enough X" text.
fn badged_icon(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    group: &str,
    name: &str,
    size: f32,
    short: bool,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|slot| {
            spawn_icon(slot, icons, group, name, size);
            if short {
                slot.spawn((
                    Text::new("x"),
                    theme.font_bold((size * 0.6).max(9.0)),
                    TextColor(theme::ALARM),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-2.0),
                        bottom: Val::Px(-3.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
            }
        });
}

/// Free (uncommitted) warehouse stock across all three shelves.
fn free_stock(industry: &IndustryVm, committed: &HashMap<String, u32>, key: &str) -> u32 {
    let warehouse = &industry.warehouse;
    let total = warehouse
        .resources
        .get(key)
        .or_else(|| warehouse.materials.get(key))
        .or_else(|| warehouse.goods.get(key))
        .copied()
        .unwrap_or(0);
    total.saturating_sub(committed.get(key).copied().unwrap_or(0))
}

/// `3 [icon]` pair used by the forecast line and cost strings. `short`
/// paints the count red and badges the icon.
#[allow(clippy::too_many_arguments)]
fn qty_icon(
    row: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    group: &str,
    name: &str,
    qty: u32,
    color: Color,
    short: bool,
) {
    row.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(2.0),
            ..default()
        },
        TooltipText(if short {
            format!("{qty} {} — not enough in stock", split_camel(name))
        } else {
            format!("{qty} {}", split_camel(name))
        }),
    ))
    .with_children(|pair| {
        pair.spawn((
            Text::new(format!("{qty}")),
            theme.font_bold(12.5),
            TextColor(if short { theme::ALARM } else { color }),
        ));
        badged_icon(pair, theme, icons, group, name, 16.0, short);
    });
}

/// Non-chain facility row (late-game buildings): name, capacity, expand.
fn building_row(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    industry: &IndustryVm,
    building: &BuildingVm,
    observer: bool,
) {
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

// ── Card 2: Workforce ────────────────────────────────────────────────────

fn card_workforce(
    content: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    buildable: Option<&BuildableUnitsVm>,
    observer: bool,
) {
    content
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|columns| {
            columns.spawn(column_node()).with_children(|col| {
                family_heading(col, theme, "Labor Pool");
                section_labor(col, theme, icons, industry);
                family_heading(col, theme, "Education");
                section_education(col, theme, icons, industry, observer);
            });
            columns.spawn(column_node()).with_children(|col| {
                family_heading(col, theme, "Immigration");
                section_immigration(col, theme, icons, industry, observer);
                if let Some(buildable) = buildable {
                    section_civilians(col, theme, icons, industry, buildable, observer);
                }
            });
        });
}

/// One labor tier: pictogram, name, free (total) count.
fn tier_row(
    card: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    icon_name: &str,
    label: &str,
    count: u32,
    committed: u32,
) {
    let free = count.saturating_sub(committed);
    card.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        margin: UiRect::bottom(Val::Px(2.0)),
        ..default()
    })
    .with_children(|row| {
        row.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|left| {
            spawn_icon(left, icons, "ui", icon_name, 20.0);
            left.spawn((
                Text::new(label.to_string()),
                theme.font(13.0),
                TextColor(theme::TEXT),
            ));
        });
        let value = if committed > 0 {
            format!("{free} ({count})")
        } else {
            format!("{count}")
        };
        let mut text = row.spawn((
            Text::new(value),
            theme.font_bold(14.0),
            TextColor(if committed > 0 {
                COMMITTED_BLUE
            } else {
                theme::GOLD
            }),
        ));
        if committed > 0 {
            text.insert(TooltipText(format!(
                "{free} free of {count} — {committed} committed to queued orders"
            )));
        }
    });
}

fn section_labor(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
) {
    let labor = &industry.labor;
    let committed_units = labor.committed_labor_units + production_labor(industry);
    let free_units = labor.total_labor_units.saturating_sub(committed_units);
    col.spawn(inset_panel()).with_children(|card| {
        tier_row(
            card,
            theme,
            icons,
            "WorkerUntrained",
            "Untrained",
            labor.untrained,
            labor.committed_untrained,
        );
        tier_row(
            card,
            theme,
            icons,
            "WorkerTrained",
            "Trained",
            labor.trained,
            labor.committed_trained,
        );
        tier_row(
            card,
            theme,
            icons,
            "WorkerExpert",
            "Expert",
            labor.expert,
            labor.committed_expert,
        );
        card.spawn((
            Text::new(format!(
                "= {free_units} free of {} labor units",
                labor.total_labor_units
            )),
            theme.font_bold(12.5),
            TextColor(theme::GOLD),
            Node {
                margin: UiRect::top(Val::Px(4.0)),
                ..default()
            },
            TooltipText(
                "Trained workers contribute 2 labor units, experts 4; production, \
                 training, and builds all draw from this pool"
                    .into(),
            ),
        ));
    });
}

/// `[from] → [to]` promotion header with the per-head cost in icon math.
#[allow(clippy::too_many_arguments)]
fn promotion_row(
    card: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    from_icon: &str,
    to_icon: &str,
    paper_cost: u32,
    labor_cost: u32,
    paper_short: bool,
) {
    card.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        ..default()
    })
    .with_children(|row| {
        row.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|diagram| {
            spawn_icon(diagram, icons, "ui", from_icon, 20.0);
            diagram.spawn((Text::new("→"), theme.font(14.0), TextColor(theme::TEXT_DIM)));
            spawn_icon(diagram, icons, "ui", to_icon, 20.0);
        });
        row.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(3.0),
            ..default()
        })
        .with_children(|cost| {
            qty_icon(
                cost,
                theme,
                icons,
                "commodities",
                "Paper",
                paper_cost,
                theme::TEXT,
                paper_short,
            );
            plus_sign(cost, theme);
            qty_icon(
                cost,
                theme,
                icons,
                "ui",
                "Workers",
                labor_cost,
                theme::TEXT,
                false,
            );
            cost.spawn((
                Text::new("each"),
                theme.font(10.5),
                TextColor(theme::TEXT_DIM),
            ));
        });
    });
}

fn section_education(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    observer: bool,
) {
    let labor = &industry.labor;
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
    let paper_needed =
        pending.to_trained * costs.to_trained_paper + pending.to_expert * costs.to_expert_paper;
    // Badge the paper icon whenever paper (not workers) is what caps training.
    let trained_short = max_by_paper < labor.untrained || paper_needed > paper;
    let expert_short = paper < paper_needed + costs.to_expert_paper && labor.trained > 0;
    col.spawn(inset_panel()).with_children(|card| {
        promotion_row(
            card,
            theme,
            icons,
            "WorkerUntrained",
            "WorkerTrained",
            costs.to_trained_paper,
            costs.to_trained_labor,
            trained_short,
        );
        spawn_action_slider(
            card,
            theme,
            max_to_trained,
            pending.to_trained,
            IndustryAction::TrainToTrained,
            observer,
        );
        promotion_row(
            card,
            theme,
            icons,
            "WorkerTrained",
            "WorkerExpert",
            costs.to_expert_paper,
            costs.to_expert_labor,
            expert_short,
        );
        spawn_action_slider(
            card,
            theme,
            labor.trained,
            pending.to_expert,
            IndustryAction::TrainToExpert,
            observer,
        );
        if trained_short || expert_short {
            jump_link(
                card,
                theme,
                "Queue Lumber → Paper on Production",
                IndustryCard::Production,
            );
        }
    });
}

fn section_immigration(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    observer: bool,
) {
    let costs = &industry.immigration_costs;
    let warehouse = &industry.warehouse;
    let stock = |key: &str| {
        warehouse
            .materials
            .get(key)
            .or_else(|| warehouse.goods.get(key))
            .copied()
            .unwrap_or(0)
    };
    let canned_short = stock("CannedFood") < costs.canned_food;
    let clothing_short = stock("Clothing") < costs.clothing;
    col.spawn(inset_panel()).with_children(|card| {
        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|left| {
                spawn_icon(left, icons, "ui", "WorkerUntrained", 20.0);
                left.spawn((
                    Text::new("New workers"),
                    theme.font(13.0),
                    TextColor(theme::TEXT),
                ));
            });
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(3.0),
                ..default()
            })
            .with_children(|cost| {
                qty_icon(
                    cost,
                    theme,
                    icons,
                    "commodities",
                    "CannedFood",
                    costs.canned_food,
                    theme::TEXT,
                    canned_short,
                );
                plus_sign(cost, theme);
                qty_icon(
                    cost,
                    theme,
                    icons,
                    "commodities",
                    "Clothing",
                    costs.clothing,
                    theme::TEXT,
                    clothing_short,
                );
                cost.spawn((
                    Text::new("each"),
                    theme.font(10.5),
                    TextColor(theme::TEXT_DIM),
                ));
            });
        });
        let show = industry.max_pending_immigration > 0 || industry.pending_immigration > 0;
        if show {
            let max = industry
                .max_pending_immigration
                .max(industry.pending_immigration);
            spawn_action_slider(
                card,
                theme,
                max,
                industry.pending_immigration,
                IndustryAction::Immigration,
                observer,
            );
        } else if canned_short || clothing_short {
            jump_link(
                card,
                theme,
                "Queue Canned Food and Clothing on Production",
                IndustryCard::Production,
            );
        } else {
            // Stocked but still blocked: the constraint is worker slots.
            card.spawn((
                Text::new("No open worker slots"),
                theme.font(11.0),
                TextColor(theme::MUTED),
            ));
        }
    });
}

fn section_civilians(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    buildable: &BuildableUnitsVm,
    observer: bool,
) {
    let civilians: Vec<&BuildableEntryVm> =
        buildable.civilians.iter().filter(|b| b.tech_met).collect();
    if civilians.is_empty() {
        return;
    }
    family_heading(col, theme, "Civilian Hiring");
    let labor = &industry.labor;
    let expert_free = labor.expert.saturating_sub(labor.committed_expert);
    col.spawn(inset_panel()).with_children(|card| {
        for entry in civilians {
            let pending = industry
                .pending_civilian_hires
                .get(&entry.unit_type)
                .copied()
                .unwrap_or(0);
            let expert = entry.expert_required.unwrap_or(false);
            let expert_short = expert && expert_free == 0;
            let money_short = entry.cost.unwrap_or(0) > buildable.treasury;
            let items: Vec<(u32, &str, &str, bool)> = if expert {
                vec![(1, "ui", "WorkerExpert", expert_short)]
            } else {
                Vec::new()
            };
            build_row(
                card,
                theme,
                icons,
                "civilians",
                &entry.unit_type,
                &entry.unit_type,
                entry.cost,
                money_short,
                &items,
                None,
                entry.max_count,
                pending,
                entry.tech_met,
                IndustryAction::Hire(entry.unit_type.clone()),
                observer,
            );
        }
    });
}

/// Flat inline link that pages to another card (same handler as the tabs).
fn jump_link(col: &mut ChildSpawnerCommands, theme: &Theme, label: &str, card: IndustryCard) {
    let button = widgets::spawn_button(
        col,
        theme,
        ButtonProps {
            label: format!("{label} →"),
            font_size: 11.0,
            flat: true,
            ..default()
        },
    );
    let mut commands = col.commands();
    let mut entity = commands.entity(button);
    entity.insert((
        CardTabButton(card),
        Node {
            height: Val::Px(22.0),
            align_items: AlignItems::Center,
            align_self: AlignSelf::FlexStart,
            padding: UiRect::horizontal(Val::Px(2.0)),
            ..default()
        },
        TooltipText(format!(
            "Go to the {} card (hotkey {})",
            card.title(),
            card.index() + 1
        )),
    ));
}

// ── Card 3: Warehouse ────────────────────────────────────────────────────

fn card_warehouse(
    content: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    ui: &IndustryUi,
) {
    let checkbox = widgets::spawn_checkbox(
        content,
        theme,
        CheckboxProps {
            label: "Debug: show AI targets".into(),
            checked: ui.show_targets,
            enabled: true,
        },
    );
    content.commands_mut().entity(checkbox).insert((
        ShowTargetsCheckbox,
        TooltipText(
            "Show the AI's per-commodity stock target beside each entry. Resources use the \
             buy-side stockpile target; materials/goods use the sell-side reserve."
                .into(),
        ),
    ));

    content
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(14.0),
            width: Val::Percent(100.0),
            margin: UiRect::top(Val::Px(6.0)),
            ..default()
        })
        .with_children(|grid| {
            grid.spawn(column_node()).with_children(|third| {
                warehouse_group(
                    third,
                    theme,
                    icons,
                    "Resources",
                    &industry.warehouse.resources,
                    &industry.warehouse_targets.resources,
                    committed,
                    ui.show_targets,
                );
            });
            grid.spawn(column_node()).with_children(|third| {
                warehouse_group(
                    third,
                    theme,
                    icons,
                    "Materials",
                    &industry.warehouse.materials,
                    &industry.warehouse_targets.materials,
                    committed,
                    ui.show_targets,
                );
            });
            grid.spawn(column_node()).with_children(|third| {
                warehouse_group(
                    third,
                    theme,
                    icons,
                    "Goods",
                    &industry.warehouse.goods,
                    &industry.warehouse_targets.goods,
                    committed,
                    ui.show_targets,
                );
            });
        });
}

#[allow(clippy::too_many_arguments)]
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
    // counts right-aligned. This card doubles as the icon legend, so the
    // commodity names stay spelled out here.
    parent.spawn(inset_panel()).with_children(|card| {
        card.spawn((
            Text::new(title.to_uppercase()),
            theme.font(11.0),
            TextColor(theme::TEXT_DIM),
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
        ));
        if keys.is_empty() {
            card.spawn((
                Text::new("Empty"),
                theme.font_italic(11.5),
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
                margin: UiRect::bottom(Val::Px(3.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|left| {
                    spawn_icon(left, icons, "commodities", key, 20.0);
                    left.spawn((
                        Text::new(split_camel(key)),
                        theme.font(13.5),
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
                                theme.font(11.0),
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
                                theme.font(11.5),
                                TextColor(theme::TEXT_DIM),
                            ));
                        }
                        right.spawn((
                            Text::new(format!("{free}")),
                            theme.font_bold(14.0),
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
/// immigration / freight / recruits / ships — the web panel's `committed`
/// map, extended with pending ship construction costs (the backend does not
/// deduct those from stock until the turn resolves).
fn committed_map(
    industry: &IndustryVm,
    buildable: Option<&BuildableUnitsVm>,
) -> HashMap<String, u32> {
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
    // Queued ships, costed via the buildable VM's per-type resource needs.
    if let Some(buildable) = buildable {
        for ship in &industry.pending_ships {
            let needs = buildable
                .ships
                .iter()
                .find(|entry| entry.unit_type == *ship)
                .and_then(|entry| entry.resources_needed.as_ref());
            if let Some(needs) = needs {
                for (key, qty) in needs {
                    add(key, *qty);
                }
            }
        }
    }
    map
}

// ── Card 4: Recruit ──────────────────────────────────────────────────────

fn card_recruit(
    content: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    buildable: Option<&BuildableUnitsVm>,
    observer: bool,
) {
    content
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|columns| {
            columns.spawn(column_node()).with_children(|col| {
                if let Some(buildable) = buildable {
                    section_army(col, theme, icons, industry, buildable, observer);
                }
            });
            columns.spawn(column_node()).with_children(|col| {
                if let Some(buildable) = buildable {
                    section_naval(col, theme, icons, industry, committed, buildable, observer);
                }
            });
            columns.spawn(column_node()).with_children(|col| {
                section_logistics(col, theme, icons, industry, committed, observer);
            });
        });
}

fn section_logistics(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    observer: bool,
) {
    family_heading(col, theme, "Logistics");
    let blocked = industry.max_freight_cars == 0 && industry.pending_freight_cars == 0;
    let lumber_short = blocked && free_stock(industry, committed, "Lumber") == 0;
    let steel_short = blocked && free_stock(industry, committed, "Steel") == 0;
    col.spawn(inset_panel()).with_children(|card| {
        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|left| {
                spawn_icon(left, icons, "ui", "FreightCar", 20.0);
                left.spawn((
                    Text::new("Freight Cars"),
                    theme.font(13.0),
                    TextColor(theme::TEXT),
                ));
            });
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(3.0),
                ..default()
            })
            .with_children(|cost| {
                qty_icon(
                    cost,
                    theme,
                    icons,
                    "commodities",
                    "Lumber",
                    1,
                    theme::TEXT,
                    lumber_short,
                );
                plus_sign(cost, theme);
                qty_icon(
                    cost,
                    theme,
                    icons,
                    "commodities",
                    "Steel",
                    1,
                    theme::TEXT,
                    steel_short,
                );
                plus_sign(cost, theme);
                qty_icon(cost, theme, icons, "ui", "Workers", 2, theme::TEXT, false);
                cost.spawn((
                    Text::new("each"),
                    theme.font(10.5),
                    TextColor(theme::TEXT_DIM),
                ));
            });
        });
        if blocked {
            if lumber_short || steel_short {
                jump_link(
                    card,
                    theme,
                    "Queue Lumber and Steel on Production",
                    IndustryCard::Production,
                );
            }
        } else {
            spawn_action_slider(
                card,
                theme,
                industry.max_freight_cars.max(industry.pending_freight_cars),
                industry.pending_freight_cars,
                IndustryAction::FreightCars,
                observer,
            );
        }
    });
}

fn section_army(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    buildable: &BuildableUnitsVm,
    observer: bool,
) {
    // Every unit renders through build_row: recruitable rows get the queue
    // slider, blocked rows stay muted with shortage badges on their costs.
    // When the whole list is locked by an empty armory, the panel ends with
    // the unblock links.
    let army: Vec<&BuildableEntryVm> = buildable.army.iter().filter(|b| b.tech_met).collect();
    if army.is_empty() {
        return;
    }
    family_heading(col, theme, "Army");
    let queued_any = army
        .iter()
        .any(|entry| industry.pending_army_recruits.contains(&entry.unit_type));
    let locked = army.iter().all(|entry| entry.max_count == 0) && !queued_any;
    col.spawn(inset_panel()).with_children(|card| {
        for entry in &army {
            let queued = industry
                .pending_army_recruits
                .iter()
                .filter(|s| **s == entry.unit_type)
                .count() as u32;
            let arms = entry.arms_required.unwrap_or(0);
            // `buildable.arms` is already net of queued recruits (the
            // backend deducts pending_army_recruits) — no further deduction.
            let arms_short = arms > buildable.arms;
            let money_short = entry.cost.unwrap_or(0) > buildable.treasury;
            // Horses / fuel / worker-tier blockers have no cost icon in the
            // VM — fall back to the backend's reason text whenever the
            // badges can't explain why the row is greyed.
            let fallback = if entry.max_count == 0 && queued == 0 && !money_short && !arms_short {
                entry.reason.clone()
            } else {
                None
            };
            let icon = unit_icon_name(entry.category.as_deref().unwrap_or(""));
            build_row(
                card,
                theme,
                icons,
                "units",
                icon,
                &split_camel(&entry.unit_type),
                entry.cost,
                money_short,
                &[(arms, "commodities", "Arms", arms_short)],
                fallback,
                entry.max_count,
                queued,
                entry.tech_met,
                IndustryAction::Recruit(entry.unit_type.clone()),
                observer,
            );
        }
        if locked && buildable.arms == 0 {
            jump_link(
                card,
                theme,
                "Queue Steel → Arms on Production",
                IndustryCard::Production,
            );
            card.spawn((
                Text::new("or buy Arms on the Trade screen (F5)"),
                theme.font_italic(10.5),
                TextColor(theme::TEXT_DIM),
            ));
        }
    });
}

fn section_naval(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    industry: &IndustryVm,
    committed: &HashMap<String, u32>,
    buildable: &BuildableUnitsVm,
    observer: bool,
) {
    let ships: Vec<&BuildableEntryVm> = buildable.ships.iter().filter(|b| b.tech_met).collect();
    if ships.is_empty() {
        return;
    }
    family_heading(col, theme, "Navy");
    col.spawn(inset_panel()).with_children(|card| {
        for category in ["Warship", "Merchant"] {
            let in_category: Vec<&&BuildableEntryVm> = ships
                .iter()
                .filter(|b| b.category.as_deref() == Some(category))
                .collect();
            if in_category.is_empty() {
                continue;
            }
            card.spawn((
                Text::new(format!("{}S", category.to_uppercase())),
                theme.font(10.5),
                TextColor(theme::TEXT_DIM),
                Node {
                    margin: UiRect::vertical(Val::Px(3.0)),
                    ..default()
                },
            ));
            for entry in in_category {
                let queued = industry
                    .pending_ships
                    .iter()
                    .filter(|s| **s == entry.unit_type)
                    .count() as u32;
                let items: Vec<(u32, &str, &str, bool)> = entry
                    .resources_needed
                    .as_ref()
                    .map(|needs| {
                        needs
                            .iter()
                            .map(|(k, v)| {
                                let short = *v > free_stock(industry, committed, k);
                                (*v, "commodities", k.as_str(), short)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let money = entry.cost.filter(|c| *c > 0);
                build_row(
                    card,
                    theme,
                    icons,
                    "ships",
                    &entry.unit_type,
                    &split_camel(&entry.unit_type),
                    money,
                    money.unwrap_or(0) > buildable.treasury,
                    &items,
                    None,
                    entry.max_count,
                    queued,
                    entry.tech_met,
                    IndustryAction::Ship(entry.unit_type.clone()),
                    observer,
                );
            }
        }
    });
}

// ── Shared rows / sliders ────────────────────────────────────────────────

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

/// `$500 + 1 [icon] + …` cost line. `enabled: false` renders it muted;
/// per-item shortage flags badge the icons, `money_short` paints the price
/// red — together they replace the old "not enough X" text lines.
fn cost_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    money: Option<i64>,
    money_short: bool,
    items: &[(u32, &str, &str, bool)],
    enabled: bool,
) {
    let color = if enabled {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(3.0),
            ..default()
        })
        .with_children(|row| {
            let mut first = true;
            if let Some(money) = money {
                let mut price = row.spawn((
                    Text::new(format!("${}", fmt_thousands(money))),
                    theme.font_bold(11.5),
                    TextColor(if money_short {
                        theme::ALARM
                    } else if enabled {
                        theme::GOLD
                    } else {
                        theme::TEXT_DIM
                    }),
                ));
                if money_short {
                    price.insert(TooltipText("Not enough treasury".into()));
                }
                first = false;
            }
            for (qty, group, name, short) in items {
                if *qty == 0 {
                    continue;
                }
                if !first {
                    plus_sign(row, theme);
                }
                qty_icon(row, theme, icons, group, name, *qty, color, *short);
                first = false;
            }
        });
}

/// Recruit/ship/hire row: icon + name + icon-math cost with shortage
/// badges, plus the queue slider when anything can be built (web
/// `ShipBuildRow`). A row that can't build stays muted — the red badges on
/// its cost say why; `fallback_reason` covers blockers the cost icons
/// cannot express (horses, fuel, worker tier).
#[allow(clippy::too_many_arguments)]
fn build_row(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    icon_group: &str,
    icon_name: &str,
    label: &str,
    money: Option<i64>,
    money_short: bool,
    cost_items: &[(u32, &str, &str, bool)],
    fallback_reason: Option<String>,
    max_count: u32,
    queued: u32,
    tech_met: bool,
    action: IndustryAction,
    observer: bool,
) {
    let can_build = tech_met && max_count > 0;
    let show_slider = can_build || queued > 0;
    col.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::FlexStart,
        column_gap: Val::Px(8.0),
        margin: UiRect::bottom(Val::Px(4.0)),
        ..default()
    })
    .with_children(|row| {
        spawn_icon(row, icons, icon_group, icon_name, 20.0);
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
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|line| {
                line.spawn((
                    Text::new(label.to_string()),
                    theme.font(13.0),
                    TextColor(if show_slider {
                        theme::TEXT
                    } else {
                        theme::TEXT_DIM
                    }),
                ));
                cost_row(
                    line,
                    theme,
                    icons,
                    money,
                    money_short,
                    cost_items,
                    show_slider,
                );
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
                        width: Val::Px(140.0),
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
            } else if let Some(reason) = fallback_reason {
                body.spawn((Text::new(reason), theme.font(10.5), TextColor(theme::MUTED)));
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
    closes: Query<(), With<IndustryCloseButton>>,
    tabs: Query<&CardTabButton>,
    navs: Query<&CardNavButton>,
    mut ui: ResMut<IndustryUi>,
    mut next_screen: ResMut<NextState<crate::state::Screen>>,
    mut out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(button) = expand.get(*entity) {
            out.write(GameCommand::ExpandBuilding {
                building_type: button.0.clone(),
            });
        } else if closes.contains(*entity) {
            next_screen.set(crate::state::Screen::Map);
        } else if let Ok(tab) = tabs.get(*entity) {
            if ui.active_card != tab.0 {
                ui.active_card = tab.0;
            }
        } else if let Ok(nav) = navs.get(*entity) {
            ui.active_card = ui.active_card.offset(nav.0);
        }
    }
}

/// ←/→ page through the cards (wrap-around); 1–4 jump directly. Text-input
/// focus and open modals keep the keys to themselves.
pub fn industry_card_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    modal_stack: Res<ModalStack>,
    mut ui: ResMut<IndustryUi>,
) {
    if focus.0.is_some() || !modal_stack.is_empty() {
        return;
    }
    let mut card = ui.active_card;
    if keys.just_pressed(KeyCode::ArrowLeft) {
        card = card.offset(-1);
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        card = card.offset(1);
    }
    for (key, target) in [
        (KeyCode::Digit1, IndustryCard::Production),
        (KeyCode::Digit2, IndustryCard::Workforce),
        (KeyCode::Digit3, IndustryCard::Warehouse),
        (KeyCode::Digit4, IndustryCard::Recruit),
        (KeyCode::Numpad1, IndustryCard::Production),
        (KeyCode::Numpad2, IndustryCard::Workforce),
        (KeyCode::Numpad3, IndustryCard::Warehouse),
        (KeyCode::Numpad4, IndustryCard::Recruit),
    ] {
        if keys.just_pressed(key) {
            card = target;
        }
    }
    if card != ui.active_card {
        ui.active_card = card;
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
