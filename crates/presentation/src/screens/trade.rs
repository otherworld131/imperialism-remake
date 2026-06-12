//! Trade screen (F5): full-screen overlay mirroring the web `TradeScreen`.
//!
//! Header: cargo used/total (tooltip = merchant fleet breakdown) and trade
//! balance. Orders tab: minor auto-buy toggle, sell-order sliders, the
//! filterable/sortable buy-offer table with a Buy modal, and trade-partner
//! subsidies. Historical Country tab: per-turn sidebar plus split vs
//! aggregated transaction tables. Historical Market tab: per-turn market
//! offers with sold quantities and buyers. Every action queues pending
//! state for end-turn resolution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::ui::InteractionDisabled;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, ViewModels};
use crate::game::vm::{TradeHistoryVm, TradeVm};
use crate::map::icons::IconAssets;
use crate::screens::common::{full_screen_root, spawn_icon, split_camel};
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, CheckboxProps, CheckboxToggled, ColumnSpec, ModalProps,
    ModalStack, MultiDropdownChanged, MultiDropdownProps, ScrollProps, SliderCommitted,
    SliderProps, TableProps, TooltipText, UiMultiDropdown, UiTable,
};

const IMPORT_RED: Color = Color::srgb_u8(0xe6, 0x39, 0x46);
const EXPORT_GREEN: Color = Color::srgb_u8(0x2a, 0x9d, 0x8f);

/// Commodity categories (web `resourceEmoji.ts` parity) for the quick chips.
const RESOURCES: &[&str] = &[
    "Grain",
    "Fruit",
    "Cotton",
    "Wool",
    "Timber",
    "Livestock",
    "Fish",
    "Horses",
    "Coal",
    "Iron",
    "Gold",
    "Gems",
    "Oil",
];
const MATERIALS: &[&str] = &["Lumber", "Steel", "Fabric", "Paper", "Arms", "CannedFood"];
const GOODS: &[&str] = &["Furniture", "Clothing", "Hardware"];

// ── Markers / state ──────────────────────────────────────────────────────

#[derive(Component)]
pub struct TradeRoot;

#[derive(Component)]
pub struct TradeHeaderStats;

#[derive(Component)]
pub struct SellColumn;

#[derive(Component)]
pub struct OffersFilterBar;

#[derive(Component)]
pub struct OffersTableAnchor;

#[derive(Component)]
pub struct PartnersAnchor;

#[derive(Component)]
pub struct HistSidebar;

#[derive(Component)]
pub struct HistControls;

#[derive(Component)]
pub struct HistTableAnchor;

#[derive(Component)]
pub struct MarketSidebar;

#[derive(Component)]
pub struct MarketFilterBar;

#[derive(Component)]
pub struct MarketTableAnchor;

#[derive(Component)]
pub struct TradeCloseButton;

/// Toggle button: clicking queues `SetAutoTradeWithMinors(enable)`.
#[derive(Component)]
pub struct AutoTradeToggle(pub bool);

#[derive(Component)]
pub struct SubsidyButton {
    pub nation_id: u32,
    pub amount: i64,
}

#[derive(Component)]
pub struct TradeSellSlider(pub String);

#[derive(Component)]
pub struct TradeBuyButton(pub String);

#[derive(Component)]
pub struct BuyQtySlider;

#[derive(Component)]
pub struct BuyConfirmButton;

#[derive(Component)]
pub struct CancelBuyButton;

#[derive(Component)]
pub struct HistSplitCheckbox;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilterTable {
    Offers,
    History,
    Market,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilterKind {
    Commodity,
    Country,
}

#[derive(Component)]
pub struct TradeFilterDropdown {
    pub table: FilterTable,
    pub kind: FilterKind,
}

/// Raw filter values aligned with the dropdown's options (resource names,
/// or nation ids as strings for the country filter).
#[derive(Component)]
pub struct FilterValues(pub Vec<String>);

/// Quick chip: toggles a whole group of filter values (web `QuickChip`).
#[derive(Component)]
pub struct TradeChip {
    pub table: FilterTable,
    pub kind: FilterKind,
    pub values: Vec<String>,
}

#[derive(Component)]
pub struct TurnButton {
    pub table: FilterTable,
    pub turn: u32,
}

#[derive(Clone)]
pub struct BuyModalState {
    pub resource: String,
    pub price: i64,
    pub qty: u32,
}

/// Screen-local trade UI state (filters, selected turns, buy modal).
#[derive(Resource, Default)]
pub struct TradeUi {
    pub offer_commodity: HashSet<String>,
    pub offer_country: HashSet<u32>,
    pub hist_commodity: HashSet<String>,
    pub hist_country: HashSet<u32>,
    pub market_commodity: HashSet<String>,
    pub market_country: HashSet<u32>,
    pub hist_split: bool,
    pub hist_turn: Option<u32>,
    pub market_turn: Option<u32>,
    pub buy: Option<BuyModalState>,
}

impl TradeUi {
    fn filter_set(&mut self, table: FilterTable, kind: FilterKind) -> FilterSetMut<'_> {
        match (table, kind) {
            (FilterTable::Offers, FilterKind::Commodity) => {
                FilterSetMut::Commodity(&mut self.offer_commodity)
            }
            (FilterTable::Offers, FilterKind::Country) => {
                FilterSetMut::Country(&mut self.offer_country)
            }
            (FilterTable::History, FilterKind::Commodity) => {
                FilterSetMut::Commodity(&mut self.hist_commodity)
            }
            (FilterTable::History, FilterKind::Country) => {
                FilterSetMut::Country(&mut self.hist_country)
            }
            (FilterTable::Market, FilterKind::Commodity) => {
                FilterSetMut::Commodity(&mut self.market_commodity)
            }
            (FilterTable::Market, FilterKind::Country) => {
                FilterSetMut::Country(&mut self.market_country)
            }
        }
    }
}

enum FilterSetMut<'a> {
    Commodity(&'a mut HashSet<String>),
    Country(&'a mut HashSet<u32>),
}

impl FilterSetMut<'_> {
    fn contains(&self, value: &str) -> bool {
        match self {
            FilterSetMut::Commodity(set) => set.contains(value),
            FilterSetMut::Country(set) => value
                .parse::<u32>()
                .map(|id| set.contains(&id))
                .unwrap_or(false),
        }
    }

    fn insert(&mut self, value: &str) {
        match self {
            FilterSetMut::Commodity(set) => {
                set.insert(value.to_string());
            }
            FilterSetMut::Country(set) => {
                if let Ok(id) = value.parse::<u32>() {
                    set.insert(id);
                }
            }
        }
    }

    fn remove(&mut self, value: &str) {
        match self {
            FilterSetMut::Commodity(set) => {
                set.remove(value);
            }
            FilterSetMut::Country(set) => {
                if let Ok(id) = value.parse::<u32>() {
                    set.remove(&id);
                }
            }
        }
    }

    fn replace_from(&mut self, values: impl Iterator<Item = String>) {
        match self {
            FilterSetMut::Commodity(set) => {
                **set = values.collect();
            }
            FilterSetMut::Country(set) => {
                **set = values.filter_map(|v| v.parse::<u32>().ok()).collect();
            }
        }
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────

pub fn enter_trade(mut commands: Commands, theme: Res<Theme>, mut ui: ResMut<TradeUi>) {
    *ui = TradeUi::default();
    let root = full_screen_root(&mut commands);
    commands.entity(root).insert(TradeRoot);
    commands.entity(root).with_children(|panel| {
        // Header.
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: Val::Px(16.0),
                    padding: UiRect::bottom(Val::Px(8.0)),
                    border: UiRect::bottom(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new("Trade"),
                    theme.font_bold(19.0),
                    TextColor(theme::GOLD),
                ));
                header.spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(16.0),
                        ..default()
                    },
                    TradeHeaderStats,
                ));
                let close = widgets::spawn_button(
                    header,
                    &theme,
                    ButtonProps {
                        label: "Close (Esc)".into(),
                        font_size: 12.0,
                        ..default()
                    },
                );
                header.commands().entity(close).insert(TradeCloseButton);
            });

        // Tabs.
        let tabs = widgets::spawn_tabs(
            panel,
            &theme,
            &["Orders", "Historical Country", "Historical Market"],
            0,
        );
        let mut commands = panel.commands();
        commands.entity(tabs.root).insert(Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            ..default()
        });
        for (index, tab_panel) in tabs.panels.iter().enumerate() {
            commands.entity(*tab_panel).insert(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                padding: UiRect::all(Val::Px(8.0)),
                display: if index == 0 {
                    Display::Flex
                } else {
                    Display::None
                },
                ..default()
            });
        }

        // Orders tab.
        commands.entity(tabs.panels[0]).with_children(|orders| {
            orders
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(18.0),
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    ..default()
                })
                .with_children(|row| {
                    let sell = widgets::spawn_scroll_area(
                        row,
                        &theme,
                        ScrollProps {
                            width: Val::Percent(40.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                    );
                    row.commands().entity(sell.content).insert(SellColumn);
                    let buy = widgets::spawn_scroll_area(
                        row,
                        &theme,
                        ScrollProps {
                            width: Val::Percent(60.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                    );
                    row.commands().entity(buy.content).with_children(|col| {
                        col.spawn((
                            Text::new("Buy Orders"),
                            theme.font_bold(15.0),
                            TextColor(theme::GOLD),
                        ));
                        col.spawn((
                            Text::new("AVAILABLE ON MARKET"),
                            theme.font(9.5),
                            TextColor(theme::TEXT_DIM),
                            Node {
                                margin: UiRect::vertical(Val::Px(4.0)),
                                ..default()
                            },
                        ));
                        col.spawn((filter_bar_node(), OffersFilterBar));
                        col.spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                width: Val::Percent(100.0),
                                ..default()
                            },
                            OffersTableAnchor,
                        ));
                        col.spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                width: Val::Percent(100.0),
                                margin: UiRect::top(Val::Px(14.0)),
                                ..default()
                            },
                            PartnersAnchor,
                        ));
                    });
                });
        });

        // Historical Country tab.
        commands.entity(tabs.panels[1]).with_children(|hist| {
            hist.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                ..default()
            })
            .with_children(|row| {
                let sidebar = widgets::spawn_scroll_area(
                    row,
                    &theme,
                    ScrollProps {
                        width: Val::Px(140.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                );
                row.commands().entity(sidebar.content).insert(HistSidebar);
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    min_height: Val::Px(0.0),
                    ..default()
                })
                .with_children(|content| {
                    content.spawn((filter_bar_node(), HistControls));
                    let table = widgets::spawn_scroll_area(
                        content,
                        &theme,
                        ScrollProps {
                            flex_grow: 1.0,
                            ..default()
                        },
                    );
                    content
                        .commands()
                        .entity(table.content)
                        .insert(HistTableAnchor);
                });
            });
        });

        // Historical Market tab.
        commands.entity(tabs.panels[2]).with_children(|market| {
            market
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    ..default()
                })
                .with_children(|row| {
                    let sidebar = widgets::spawn_scroll_area(
                        row,
                        &theme,
                        ScrollProps {
                            width: Val::Px(140.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                    );
                    row.commands().entity(sidebar.content).insert(MarketSidebar);
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        min_width: Val::Px(0.0),
                        min_height: Val::Px(0.0),
                        ..default()
                    })
                    .with_children(|content| {
                        content.spawn((filter_bar_node(), MarketFilterBar));
                        let table = widgets::spawn_scroll_area(
                            content,
                            &theme,
                            ScrollProps {
                                flex_grow: 1.0,
                                ..default()
                            },
                        );
                        content
                            .commands()
                            .entity(table.content)
                            .insert(MarketTableAnchor);
                    });
                });
        });
    });
}

fn filter_bar_node() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::Center,
        column_gap: Val::Px(6.0),
        row_gap: Val::Px(4.0),
        margin: UiRect::bottom(Val::Px(8.0)),
        ..default()
    }
}

pub fn exit_trade(mut commands: Commands, roots: Query<Entity, With<TradeRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

// ── Rebuild: header / sell column / static filter bars / partners ────────

pub fn update_trade_static(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    ui: Res<TradeUi>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    added: Query<(), Added<TradeRoot>>,
    header: Query<Entity, With<TradeHeaderStats>>,
    sell: Query<Entity, With<SellColumn>>,
    offers_bar: Query<Entity, With<OffersFilterBar>>,
    partners: Query<Entity, With<PartnersAnchor>>,
    hist_controls: Query<Entity, With<HistControls>>,
) {
    if !vms.is_changed() && added.is_empty() {
        return;
    }
    let Ok(header) = header.single() else {
        return;
    };
    let Some(trade) = vms.trade.as_ref() else {
        return;
    };
    let icons = icons.as_deref();
    let observer = meta.observer;

    // Header stats.
    commands.entity(header).despawn_children();
    commands.entity(header).with_children(|stats| {
        let used = trade.total_cargo - trade.remaining_cargo;
        let merchants = vms
            .ships
            .as_ref()
            .map(|s| s.merchants.as_slice())
            .unwrap_or(&[]);
        let mut counts: HashMap<&str, u32> = HashMap::new();
        for ship in merchants {
            *counts.entry(ship.ship_type.as_str()).or_insert(0) += 1;
        }
        let mut breakdown: Vec<(&str, u32)> = counts.into_iter().collect();
        breakdown.sort();
        let tooltip = if breakdown.is_empty() {
            "No merchant ships".to_string()
        } else {
            format!(
                "Merchant fleet ({}): {}",
                merchants.len(),
                breakdown
                    .iter()
                    .map(|(ship_type, count)| format!("{} ×{count}", split_camel(ship_type)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let cargo = stats
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|row| {
                spawn_icon(row, icons, "ships", "Trader", 16.0);
                row.spawn((
                    Text::new(format!("Cargo: {used} / {}", trade.total_cargo)),
                    theme.font(13.0),
                    TextColor(theme::TEXT),
                ));
            })
            .id();
        stats
            .commands()
            .entity(cargo)
            .insert((TooltipText(tooltip), Interaction::default()));

        header_stat(
            stats,
            &theme,
            "Imports:",
            trade.trade_balance.total_bought,
            IMPORT_RED,
        );
        header_stat(
            stats,
            &theme,
            "Exports:",
            trade.trade_balance.total_sold,
            EXPORT_GREEN,
        );
        header_stat(
            stats,
            &theme,
            "Net:",
            trade.trade_balance.net,
            if trade.trade_balance.net >= 0 {
                EXPORT_GREEN
            } else {
                IMPORT_RED
            },
        );
    });

    // Sell column.
    if let Ok(sell) = sell.single() {
        commands.entity(sell).despawn_children();
        commands.entity(sell).with_children(|col| {
            build_sell_column(col, &theme, icons, trade, observer);
        });
    }

    // Offer filters.
    if let Ok(bar) = offers_bar.single() {
        commands.entity(bar).despawn_children();
        let mut commodities: Vec<String> = trade
            .available_offers
            .iter()
            .map(|o| o.resource.clone())
            .collect();
        commodities.sort();
        commodities.dedup();
        let mut countries: Vec<(u32, String, bool)> = Vec::new();
        for offer in &trade.available_offers {
            if !countries.iter().any(|(id, _, _)| *id == offer.seller_id) {
                countries.push((
                    offer.seller_id,
                    offer.seller_name.clone(),
                    offer.is_great_power,
                ));
            }
        }
        countries.sort_by(|a, b| a.1.cmp(&b.1));
        commands.entity(bar).with_children(|bar| {
            build_filter_bar(
                bar,
                &theme,
                FilterTable::Offers,
                &commodities,
                &countries,
                &ui.offer_commodity,
                &ui.offer_country,
            );
        });
    }

    // Trade partners.
    if let Ok(anchor) = partners.single() {
        commands.entity(anchor).despawn_children();
        commands.entity(anchor).with_children(|col| {
            col.spawn((
                Text::new("Trade Partners"),
                theme.font_bold(15.0),
                TextColor(theme::GOLD),
                Node {
                    margin: UiRect::bottom(Val::Px(6.0)),
                    ..default()
                },
            ));
            let subsidy_of = |nation_id: u32| {
                trade
                    .subsidies
                    .iter()
                    .find(|s| s.nation_id == nation_id)
                    .map(|s| s.amount)
                    .unwrap_or(0)
            };
            for minor in &trade.minor_nations {
                let current = subsidy_of(minor.nation_id);
                col.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    margin: UiRect::bottom(Val::Px(2.0)),
                    ..default()
                })
                .with_children(|row| {
                    let name = if minor.has_consulate {
                        format!("{} •", minor.name)
                    } else {
                        minor.name.clone()
                    };
                    row.spawn((Text::new(name), theme.font(12.5), TextColor(theme::TEXT)));
                    row.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|buttons| {
                        for amount in [0i64, 500, 1000, 2000] {
                            let active = current == amount;
                            let button = widgets::spawn_button(
                                buttons,
                                &theme,
                                ButtonProps {
                                    label: format!("${amount}"),
                                    font_size: 10.5,
                                    enabled: !observer,
                                    auto_label_tint: !active,
                                    ..default()
                                },
                            );
                            let mut entity = buttons.commands_mut().entity(button);
                            entity.insert(SubsidyButton {
                                nation_id: minor.nation_id,
                                amount,
                            });
                            if active {
                                entity.insert(BorderColor::all(theme::GOLD));
                            }
                        }
                    });
                });
            }
        });
    }

    // History controls (split toggle + filters).
    if let Ok(controls) = hist_controls.single() {
        commands.entity(controls).despawn_children();
        let mut commodities: Vec<String> = trade
            .trade_history
            .iter()
            .map(|h| h.resource.clone())
            .collect();
        commodities.sort();
        commodities.dedup();
        let mut countries: Vec<(u32, String, bool)> = Vec::new();
        for entry in &trade.trade_history {
            if !countries.iter().any(|(id, _, _)| *id == entry.partner_id) {
                countries.push((
                    entry.partner_id,
                    entry.partner_name.clone(),
                    entry.partner_is_great_power,
                ));
            }
        }
        countries.sort_by(|a, b| a.1.cmp(&b.1));
        commands.entity(controls).with_children(|bar| {
            let checkbox = widgets::spawn_checkbox(
                bar,
                &theme,
                CheckboxProps {
                    label: "Split individual transactions".into(),
                    checked: ui.hist_split,
                    enabled: true,
                },
            );
            bar.commands().entity(checkbox).insert(HistSplitCheckbox);
            build_filter_bar(
                bar,
                &theme,
                FilterTable::History,
                &commodities,
                &countries,
                &ui.hist_commodity,
                &ui.hist_country,
            );
        });
    }
}

fn header_stat(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    label: &str,
    value: i64,
    color: Color,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label.to_string()),
                theme.font(13.0),
                TextColor(theme::TEXT),
            ));
            row.spawn((
                Text::new(format!("${value}")),
                theme.font_bold(13.0),
                TextColor(color),
            ));
        });
}

fn build_sell_column(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    trade: &TradeVm,
    observer: bool,
) {
    col.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        margin: UiRect::bottom(Val::Px(6.0)),
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Text::new("Sell Orders"),
            theme.font_bold(15.0),
            TextColor(theme::GOLD),
        ));
        let enabled = trade.auto_trade_with_minors;
        let toggle = widgets::spawn_button(
            row,
            theme,
            ButtonProps {
                label: format!("Minor auto-buy: {}", if enabled { "ON" } else { "OFF" }),
                font_size: 10.5,
                enabled: !observer,
                auto_label_tint: false,
                ..default()
            },
        );
        let mut entity = row.commands_mut().entity(toggle);
        entity.insert((
            AutoTradeToggle(!enabled),
            TooltipText(
                "When enabled, minor nations may automatically buy your goods each turn".into(),
            ),
        ));
        entity.insert(BorderColor::all(if enabled {
            EXPORT_GREEN
        } else {
            IMPORT_RED
        }));
    });

    col.spawn((
        Text::new("RESOURCES"),
        theme.font(9.5),
        TextColor(theme::TEXT_DIM),
        Node {
            margin: UiRect::bottom(Val::Px(4.0)),
            ..default()
        },
    ));
    let order_qty: HashMap<&str, u32> = trade
        .player_sell_orders
        .iter()
        .map(|o| (o.commodity_name.as_str(), o.quantity))
        .collect();
    for item in &trade.sellable_resources {
        let qty = order_qty.get(item.name.as_str()).copied().unwrap_or(0);
        col.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            padding: UiRect::vertical(Val::Px(3.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                width: Val::Px(110.0),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|name| {
                spawn_icon(name, icons, "commodities", &item.name, 14.0);
                name.spawn((
                    Text::new(split_camel(&item.name)),
                    theme.font(12.5),
                    TextColor(theme::TEXT),
                ));
            });
            row.spawn((
                Text::new(format!("x{}", item.stock)),
                theme.font(11.5),
                TextColor(theme::TEXT_DIM),
                Node {
                    width: Val::Px(36.0),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            row.spawn((
                Text::new(format!("${}", item.price)),
                theme.font(11.5),
                TextColor(theme::GOLD),
                Node {
                    width: Val::Px(44.0),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            let slider = widgets::spawn_slider(
                row,
                theme,
                SliderProps {
                    min: 0.0,
                    max: item.stock as f32,
                    step: 1.0,
                    value: qty as f32,
                    width: Val::Px(120.0),
                    ..default()
                },
            );
            let mut entity = row.commands_mut().entity(slider);
            entity.insert(TradeSellSlider(item.name.clone()));
            if observer {
                entity.insert(InteractionDisabled);
            }
        });
    }
    if trade.sellable_resources.is_empty() {
        col.spawn((
            Text::new("Nothing in stock to sell"),
            theme.font_italic(12.0),
            TextColor(theme::TEXT_DIM),
        ));
    }
}

/// Commodity + country multi-select dropdowns with quick chips.
fn build_filter_bar(
    bar: &mut ChildSpawnerCommands,
    theme: &Theme,
    table: FilterTable,
    commodities: &[String],
    countries: &[(u32, String, bool)],
    selected_commodities: &HashSet<String>,
    selected_countries: &HashSet<u32>,
) {
    // Commodity dropdown.
    let selected: Vec<bool> = commodities
        .iter()
        .map(|c| selected_commodities.contains(c))
        .collect();
    let dropdown = widgets::spawn_multi_dropdown(
        bar,
        theme,
        MultiDropdownProps {
            label: "Commodities".into(),
            options: commodities.to_vec(),
            selected,
            width: Val::Px(170.0),
        },
    );
    bar.commands().entity(dropdown).insert((
        TradeFilterDropdown {
            table,
            kind: FilterKind::Commodity,
        },
        FilterValues(commodities.to_vec()),
    ));
    for (label, group) in [
        ("Goods", GOODS),
        ("Materials", MATERIALS),
        ("Resources", RESOURCES),
    ] {
        let values: Vec<String> = commodities
            .iter()
            .filter(|c| group.contains(&c.as_str()))
            .cloned()
            .collect();
        spawn_chip(bar, theme, table, FilterKind::Commodity, label, values);
    }

    // Country dropdown.
    let options: Vec<String> = countries.iter().map(|(_, name, _)| name.clone()).collect();
    let values: Vec<String> = countries.iter().map(|(id, _, _)| id.to_string()).collect();
    let selected: Vec<bool> = countries
        .iter()
        .map(|(id, _, _)| selected_countries.contains(id))
        .collect();
    let dropdown = widgets::spawn_multi_dropdown(
        bar,
        theme,
        MultiDropdownProps {
            label: "Countries".into(),
            options,
            selected,
            width: Val::Px(170.0),
        },
    );
    bar.commands().entity(dropdown).insert((
        TradeFilterDropdown {
            table,
            kind: FilterKind::Country,
        },
        FilterValues(values),
    ));
    let great: Vec<String> = countries
        .iter()
        .filter(|(_, _, gp)| *gp)
        .map(|(id, _, _)| id.to_string())
        .collect();
    let minor: Vec<String> = countries
        .iter()
        .filter(|(_, _, gp)| !*gp)
        .map(|(id, _, _)| id.to_string())
        .collect();
    spawn_chip(
        bar,
        theme,
        table,
        FilterKind::Country,
        "Great Powers",
        great,
    );
    spawn_chip(
        bar,
        theme,
        table,
        FilterKind::Country,
        "Minor Powers",
        minor,
    );
}

fn spawn_chip(
    bar: &mut ChildSpawnerCommands,
    theme: &Theme,
    table: FilterTable,
    kind: FilterKind,
    label: &str,
    values: Vec<String>,
) {
    if values.is_empty() {
        return;
    }
    let button = widgets::spawn_button(
        bar,
        theme,
        ButtonProps {
            label: format!("{label} ({})", values.len()),
            font_size: 10.5,
            ..default()
        },
    );
    bar.commands().entity(button).insert((
        TradeChip {
            table,
            kind,
            values,
        },
        TooltipText(format!("Toggle all {label} in the filter")),
    ));
}

// ── Rebuild: tables + sidebars (vms or filter state changed) ─────────────

pub fn update_trade_tables(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    ui: Res<TradeUi>,
    theme: Res<Theme>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    added: Query<(), Added<TradeRoot>>,
    offers_anchor: Query<Entity, With<OffersTableAnchor>>,
    hist_sidebar: Query<Entity, With<HistSidebar>>,
    hist_anchor: Query<Entity, With<HistTableAnchor>>,
    market_sidebar: Query<Entity, With<MarketSidebar>>,
    market_bar: Query<Entity, With<MarketFilterBar>>,
    market_anchor: Query<Entity, With<MarketTableAnchor>>,
    tables: Query<(&ChildOf, &UiTable)>,
    mut last_market_turn: Local<Option<Option<u32>>>,
) {
    if !vms.is_changed() && !ui.is_changed() && added.is_empty() {
        return;
    }
    let Ok(offers_anchor) = offers_anchor.single() else {
        return;
    };
    let Some(trade) = vms.trade.as_ref() else {
        return;
    };
    let icons = icons.as_deref();
    let observer = meta.observer;
    let prev_sort = |anchor: Entity| {
        tables
            .iter()
            .find(|(child_of, _)| child_of.parent() == anchor)
            .and_then(|(_, table)| table.sort)
    };

    // Offers table.
    let sort = prev_sort(offers_anchor).or(Some((0, true)));
    commands.entity(offers_anchor).despawn_children();
    commands.entity(offers_anchor).with_children(|anchor| {
        build_offers_table(anchor, &theme, icons, trade, &ui, observer, sort);
    });

    // History sidebar + table.
    if let Ok(sidebar) = hist_sidebar.single() {
        commands.entity(sidebar).despawn_children();
        commands.entity(sidebar).with_children(|col| {
            col.spawn((
                Text::new("PAST TURNS"),
                theme.font(9.5),
                TextColor(theme::TEXT_DIM),
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
            ));
            let mut turns: Vec<u32> = trade.trade_history.iter().map(|h| h.turn).collect();
            turns.sort_unstable();
            turns.dedup();
            turns.reverse();
            if turns.is_empty() {
                col.spawn((
                    Text::new("No trade history yet."),
                    theme.font_italic(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            }
            for turn in turns {
                let count = trade
                    .trade_history
                    .iter()
                    .filter(|h| h.turn == turn)
                    .count();
                let active = ui.hist_turn == Some(turn);
                turn_button(col, &theme, FilterTable::History, turn, count, active);
            }
        });
    }
    if let Ok(anchor) = hist_anchor.single() {
        let sort = prev_sort(anchor).or(Some((0, false)));
        commands.entity(anchor).despawn_children();
        commands.entity(anchor).with_children(|anchor| {
            build_history_table(anchor, &theme, icons, trade, &ui, sort);
        });
    }

    // Market sidebar + filter bar + table.
    let market_turns: Vec<u32> = trade.market_archive.iter().map(|r| r.turn).collect();
    let current_turn = ui.market_turn.or_else(|| market_turns.first().copied());
    if let Ok(sidebar) = market_sidebar.single() {
        commands.entity(sidebar).despawn_children();
        commands.entity(sidebar).with_children(|col| {
            col.spawn((
                Text::new("PAST TURNS"),
                theme.font(9.5),
                TextColor(theme::TEXT_DIM),
                Node {
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
            ));
            if market_turns.is_empty() {
                col.spawn((
                    Text::new("No market data yet."),
                    theme.font_italic(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            }
            for &turn in &market_turns {
                let count = trade
                    .market_archive
                    .iter()
                    .find(|r| r.turn == turn)
                    .map(|r| r.offers.len())
                    .unwrap_or(0);
                let active = current_turn == Some(turn);
                turn_button(col, &theme, FilterTable::Market, turn, count, active);
            }
        });
    }
    // The market filter options depend on the selected turn; only rebuild
    // the bar when the data or the turn changes so open popups survive
    // filter toggles.
    let turn_changed = *last_market_turn != Some(current_turn);
    *last_market_turn = Some(current_turn);
    if (vms.is_changed() || turn_changed || !added.is_empty())
        && let Ok(bar) = market_bar.single()
    {
        commands.entity(bar).despawn_children();
        if let Some(record) = trade
            .market_archive
            .iter()
            .find(|r| Some(r.turn) == current_turn)
        {
            let mut commodities: Vec<String> =
                record.offers.iter().map(|o| o.resource.clone()).collect();
            commodities.sort();
            commodities.dedup();
            let mut countries: Vec<(u32, String, bool)> = Vec::new();
            for offer in &record.offers {
                if !countries.iter().any(|(id, _, _)| *id == offer.seller_id) {
                    countries.push((
                        offer.seller_id,
                        offer.seller_name.clone(),
                        offer.seller_is_great_power,
                    ));
                }
                for fill in &offer.fills {
                    if !countries.iter().any(|(id, _, _)| *id == fill.buyer_id) {
                        countries.push((
                            fill.buyer_id,
                            fill.buyer_name.clone(),
                            fill.buyer_is_great_power,
                        ));
                    }
                }
            }
            countries.sort_by(|a, b| a.1.cmp(&b.1));
            commands.entity(bar).with_children(|bar| {
                build_filter_bar(
                    bar,
                    &theme,
                    FilterTable::Market,
                    &commodities,
                    &countries,
                    &ui.market_commodity,
                    &ui.market_country,
                );
            });
        }
    }
    if let Ok(anchor) = market_anchor.single() {
        let sort = prev_sort(anchor).or(Some((0, true)));
        commands.entity(anchor).despawn_children();
        commands.entity(anchor).with_children(|anchor| {
            build_market_table(anchor, &theme, icons, trade, &ui, current_turn, sort);
        });
    }
}

fn turn_button(
    col: &mut ChildSpawnerCommands,
    theme: &Theme,
    table: FilterTable,
    turn: u32,
    count: usize,
    active: bool,
) {
    let button = widgets::spawn_button(
        col,
        theme,
        ButtonProps {
            label: format!("Turn {turn}  ({count})"),
            font_size: 11.5,
            width: Some(Val::Percent(100.0)),
            auto_label_tint: !active,
            flat: !active,
            ..default()
        },
    );
    let mut entity = col.commands_mut().entity(button);
    entity.insert(TurnButton { table, turn });
    if active {
        entity.insert(BorderColor::all(theme::GOLD));
    }
}

fn build_offers_table(
    anchor: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    trade: &TradeVm,
    ui: &TradeUi,
    observer: bool,
    sort: Option<(usize, bool)>,
) {
    let rows: Vec<Vec<String>> = trade
        .available_offers
        .iter()
        .filter(|o| {
            (ui.offer_commodity.is_empty() || ui.offer_commodity.contains(&o.resource))
                && (ui.offer_country.is_empty() || ui.offer_country.contains(&o.seller_id))
        })
        .map(|o| {
            vec![
                o.resource.clone(),
                o.seller_name.clone(),
                o.quantity.to_string(),
                o.price.to_string(),
                if o.is_great_power { "1" } else { "0" }.to_string(),
                o.resource.clone(),
            ]
        })
        .collect();
    if rows.is_empty() {
        anchor.spawn((
            Text::new("No offers match the filters."),
            theme.font_italic(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        return;
    }
    let icon_map = commodity_icon_map(icons);
    let builder: widgets::CellBuilder = Arc::new(move |cell, theme, _row, col, value| match col {
        0 => commodity_cell(cell, theme, &icon_map, value),
        3 => {
            cell.spawn((
                Text::new(format!("${value}")),
                theme.font(12.5),
                TextColor(theme::GOLD),
            ));
        }
        4 => {
            cell.spawn((
                Text::new(if value == "1" { "•" } else { "" }),
                theme.font(12.5),
                TextColor(theme::GOLD),
            ));
        }
        5 => {
            let button = widgets::spawn_button(
                cell,
                theme,
                ButtonProps {
                    label: "Buy".into(),
                    font_size: 11.0,
                    enabled: !observer,
                    ..default()
                },
            );
            cell.commands()
                .entity(button)
                .insert(TradeBuyButton(value.to_string()));
        }
        _ => {
            cell.spawn((Text::new(value), theme.font(12.5), TextColor(theme::TEXT)));
        }
    });
    widgets::spawn_table(
        anchor,
        theme,
        TableProps {
            columns: vec![
                ColumnSpec::new("Item", 1.6),
                ColumnSpec::new("Seller", 1.8),
                ColumnSpec::new("Avail", 0.8),
                ColumnSpec::new("Price", 0.8),
                ColumnSpec::new("GP", 0.5),
                ColumnSpec::new("", 0.8),
            ],
            sortable: true,
            rows,
            sort,
            cell_builder: Some(builder),
        },
    );
}

fn build_history_table(
    anchor: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    trade: &TradeVm,
    ui: &TradeUi,
    sort: Option<(usize, bool)>,
) {
    if trade.trade_history.is_empty() {
        anchor.spawn((
            Text::new("No trade data available."),
            theme.font_italic(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        return;
    }
    let filtered: Vec<&TradeHistoryVm> = trade
        .trade_history
        .iter()
        .filter(|h| {
            (ui.hist_commodity.is_empty() || ui.hist_commodity.contains(&h.resource))
                && (ui.hist_country.is_empty() || ui.hist_country.contains(&h.partner_id))
                && (ui.hist_turn.is_none() || ui.hist_turn == Some(h.turn))
        })
        .collect();
    let icon_map = commodity_icon_map(icons);

    if ui.hist_split {
        let rows: Vec<Vec<String>> = filtered
            .iter()
            .map(|h| {
                vec![
                    h.turn.to_string(),
                    h.resource.clone(),
                    if h.bought { "Buy" } else { "Sell" }.to_string(),
                    h.quantity.to_string(),
                    if h.bought {
                        format!("-{}", h.total_cost)
                    } else {
                        format!("{}", h.total_cost)
                    },
                    h.partner_name.clone(),
                    if h.partner_is_great_power { "1" } else { "0" }.to_string(),
                ]
            })
            .collect();
        let builder: widgets::CellBuilder =
            Arc::new(move |cell, theme, _row, col, value| match col {
                1 => commodity_cell(cell, theme, &icon_map, value),
                2 => {
                    cell.spawn((
                        Text::new(value),
                        theme.font(12.5),
                        TextColor(if value == "Buy" {
                            IMPORT_RED
                        } else {
                            EXPORT_GREEN
                        }),
                    ));
                }
                4 => {
                    let (text, color) = signed_cost(value);
                    cell.spawn((Text::new(text), theme.font(12.5), TextColor(color)));
                }
                6 => {
                    cell.spawn((
                        Text::new(if value == "1" { "•" } else { "" }),
                        theme.font(12.5),
                        TextColor(theme::GOLD),
                    ));
                }
                _ => {
                    cell.spawn((Text::new(value), theme.font(12.5), TextColor(theme::TEXT)));
                }
            });
        widgets::spawn_table(
            anchor,
            theme,
            TableProps {
                columns: vec![
                    ColumnSpec::new("Turn", 0.6),
                    ColumnSpec::new("Item", 1.4),
                    ColumnSpec::new("B/S", 0.6),
                    ColumnSpec::new("Qty", 0.6),
                    ColumnSpec::new("Cost", 1.0),
                    ColumnSpec::new("Partner", 1.6),
                    ColumnSpec::new("GP", 0.5),
                ],
                sortable: true,
                rows,
                sort,
                cell_builder: Some(builder),
            },
        );
        return;
    }

    // Aggregated by (turn, resource, partner) — web parity.
    struct AggRow {
        turn: u32,
        resource: String,
        partner: String,
        partner_gp: bool,
        bought: u32,
        sold: u32,
        bought_cost: i64,
        sold_cost: i64,
    }
    let mut agg: Vec<AggRow> = Vec::new();
    for h in &filtered {
        let row = agg
            .iter_mut()
            .find(|r| r.turn == h.turn && r.resource == h.resource && r.partner == h.partner_name);
        let row = match row {
            Some(row) => row,
            None => {
                agg.push(AggRow {
                    turn: h.turn,
                    resource: h.resource.clone(),
                    partner: h.partner_name.clone(),
                    partner_gp: h.partner_is_great_power,
                    bought: 0,
                    sold: 0,
                    bought_cost: 0,
                    sold_cost: 0,
                });
                agg.last_mut().expect("just pushed")
            }
        };
        if h.bought {
            row.bought += h.quantity;
            row.bought_cost += h.total_cost;
        } else {
            row.sold += h.quantity;
            row.sold_cost += h.total_cost;
        }
    }
    let rows: Vec<Vec<String>> = agg
        .iter()
        .map(|r| {
            vec![
                r.turn.to_string(),
                r.resource.clone(),
                if r.partner_gp {
                    format!("{} •", r.partner)
                } else {
                    r.partner.clone()
                },
                r.bought.to_string(),
                (-r.bought_cost).to_string(),
                r.sold.to_string(),
                r.sold_cost.to_string(),
            ]
        })
        .collect();
    let builder: widgets::CellBuilder = Arc::new(move |cell, theme, _row, col, value| match col {
        1 => commodity_cell(cell, theme, &icon_map, value),
        3 | 5 => {
            let zero = value == "0";
            cell.spawn((
                Text::new(if zero {
                    "—".to_string()
                } else {
                    value.to_string()
                }),
                theme.font(12.5),
                TextColor(if zero {
                    theme::TEXT_DIM
                } else if col == 3 {
                    IMPORT_RED
                } else {
                    EXPORT_GREEN
                }),
            ));
        }
        4 | 6 => {
            if value == "0" {
                cell.spawn((Text::new("—"), theme.font(12.5), TextColor(theme::TEXT_DIM)));
            } else {
                let (text, color) = signed_cost(value);
                cell.spawn((Text::new(text), theme.font(12.5), TextColor(color)));
            }
        }
        _ => {
            cell.spawn((Text::new(value), theme.font(12.5), TextColor(theme::TEXT)));
        }
    });
    widgets::spawn_table(
        anchor,
        theme,
        TableProps {
            columns: vec![
                ColumnSpec::new("Turn", 0.6),
                ColumnSpec::new("Item", 1.4),
                ColumnSpec::new("Partner", 1.6),
                ColumnSpec::new("Bought", 0.7),
                ColumnSpec::new("Cost", 1.0),
                ColumnSpec::new("Sold", 0.7),
                ColumnSpec::new("Revenue", 1.0),
            ],
            sortable: true,
            rows,
            sort,
            cell_builder: Some(builder),
        },
    );
}

fn build_market_table(
    anchor: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    trade: &TradeVm,
    ui: &TradeUi,
    current_turn: Option<u32>,
    sort: Option<(usize, bool)>,
) {
    if trade.market_archive.is_empty() {
        anchor.spawn((
            Text::new("No market activity recorded yet."),
            theme.font_italic(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        return;
    }
    let Some(record) = trade
        .market_archive
        .iter()
        .find(|r| Some(r.turn) == current_turn)
    else {
        return;
    };
    let rows: Vec<Vec<String>> = record
        .offers
        .iter()
        .filter(|o| {
            (ui.market_commodity.is_empty() || ui.market_commodity.contains(&o.resource))
                && (ui.market_country.is_empty()
                    || ui.market_country.contains(&o.seller_id)
                    || o.fills
                        .iter()
                        .any(|f| ui.market_country.contains(&f.buyer_id)))
        })
        .map(|o| {
            let fills = if o.fills.is_empty() {
                "—".to_string()
            } else {
                o.fills
                    .iter()
                    .map(|f| format!("{} ×{}", f.buyer_name, f.quantity))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            vec![
                o.resource.clone(),
                if o.seller_is_great_power {
                    format!("{} •", o.seller_name)
                } else {
                    o.seller_name.clone()
                },
                o.offered.to_string(),
                o.sold.to_string(),
                o.price_per_unit.to_string(),
                fills,
            ]
        })
        .collect();
    if rows.is_empty() {
        anchor.spawn((
            Text::new("No offers match the filters."),
            theme.font_italic(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        return;
    }
    let icon_map = commodity_icon_map(icons);
    let builder: widgets::CellBuilder = Arc::new(move |cell, theme, _row, col, value| match col {
        0 => commodity_cell(cell, theme, &icon_map, value),
        4 => {
            cell.spawn((
                Text::new(format!("${value}")),
                theme.font(12.5),
                TextColor(theme::TEXT),
            ));
        }
        5 => {
            cell.spawn((
                Text::new(value),
                theme.font(11.5),
                TextColor(if value == "—" {
                    theme::TEXT_DIM
                } else {
                    theme::TEXT
                }),
            ));
        }
        _ => {
            cell.spawn((Text::new(value), theme.font(12.5), TextColor(theme::TEXT)));
        }
    });
    widgets::spawn_table(
        anchor,
        theme,
        TableProps {
            columns: vec![
                ColumnSpec::new("Item", 1.2),
                ColumnSpec::new("Seller", 1.5),
                ColumnSpec::new("Offered", 0.7),
                ColumnSpec::new("Sold", 0.6),
                ColumnSpec::new("Price", 0.7),
                ColumnSpec::new("Bought by", 2.2),
            ],
            sortable: true,
            rows,
            sort,
            cell_builder: Some(builder),
        },
    );
}

fn commodity_cell(
    cell: &mut ChildSpawnerCommands,
    theme: &Theme,
    icon_map: &HashMap<String, Handle<Image>>,
    value: &str,
) {
    cell.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(4.0),
        ..default()
    })
    .with_children(|row| {
        if let Some(handle) = icon_map.get(value) {
            row.spawn((
                Node {
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                ImageNode::new(handle.clone()),
            ));
        }
        row.spawn((
            Text::new(split_camel(value)),
            theme.font(12.5),
            TextColor(theme::GOLD),
        ));
    });
}

/// Snapshot the commodity icons so a `'static` cell builder can use them.
fn commodity_icon_map(icons: Option<&IconAssets>) -> HashMap<String, Handle<Image>> {
    let mut map = HashMap::new();
    if let Some(icons) = icons {
        for name in RESOURCES.iter().chain(MATERIALS).chain(GOODS) {
            if let Some(handle) = icons.get("commodities", name) {
                map.insert((*name).to_string(), handle);
            }
        }
    }
    map
}

/// `"-1200"` → `("-$1200", red)`; `"350"` → `("+$350", green)`.
fn signed_cost(value: &str) -> (String, Color) {
    if let Some(amount) = value.strip_prefix('-') {
        (format!("-${amount}"), IMPORT_RED)
    } else {
        (format!("+${value}"), EXPORT_GREEN)
    }
}

// ── Interaction ──────────────────────────────────────────────────────────

pub fn handle_trade_buttons(
    mut activations: MessageReader<ButtonActivated>,
    vms: Res<ViewModels>,
    theme: Res<Theme>,
    mut ui: ResMut<TradeUi>,
    mut commands: Commands,
    mut modal_stack: ResMut<ModalStack>,
    mut out: MessageWriter<GameCommand>,
    mut next_screen: ResMut<NextState<Screen>>,
    buttons: (
        Query<(), With<TradeCloseButton>>,
        Query<&AutoTradeToggle>,
        Query<&SubsidyButton>,
        Query<&TradeBuyButton>,
        Query<(), With<BuyConfirmButton>>,
        Query<(), With<CancelBuyButton>>,
        Query<&TurnButton>,
        Query<&TradeChip>,
    ),
    mut dropdowns: Query<(&TradeFilterDropdown, &FilterValues, &mut UiMultiDropdown)>,
) {
    let (close, auto_trade, subsidy, buy, confirm, cancel, turns, chips) = buttons;
    for ButtonActivated(entity) in activations.read() {
        let entity = *entity;
        if close.contains(entity) {
            next_screen.set(Screen::Map);
        } else if let Ok(toggle) = auto_trade.get(entity) {
            out.write(GameCommand::SetAutoTradeWithMinors { enabled: toggle.0 });
        } else if let Ok(subsidy) = subsidy.get(entity) {
            out.write(GameCommand::SetTradeSubsidy {
                nation_id: subsidy.nation_id,
                amount: subsidy.amount,
            });
        } else if let Ok(buy) = buy.get(entity) {
            open_buy_modal(
                &buy.0,
                &vms,
                &theme,
                &mut ui,
                &mut commands,
                &mut modal_stack,
            );
        } else if confirm.contains(entity) {
            if let Some(state) = ui.buy.take() {
                // Web parity: max price = ceil(offer price × 1.2).
                let max_price = (state.price * 6 + 4) / 5;
                out.write(GameCommand::SetBuyOrder {
                    resource: state.resource,
                    quantity: state.qty,
                    max_price,
                });
            }
            widgets::close_top_modal(&mut commands, &mut modal_stack);
        } else if cancel.contains(entity) {
            ui.buy = None;
            widgets::close_top_modal(&mut commands, &mut modal_stack);
        } else if let Ok(turn) = turns.get(entity) {
            match turn.table {
                FilterTable::History => {
                    ui.hist_turn = if ui.hist_turn == Some(turn.turn) {
                        None
                    } else {
                        Some(turn.turn)
                    };
                }
                FilterTable::Market => ui.market_turn = Some(turn.turn),
                FilterTable::Offers => {}
            }
        } else if let Ok(chip) = chips.get(entity) {
            let mut set = ui.filter_set(chip.table, chip.kind);
            let all_selected = chip.values.iter().all(|v| set.contains(v));
            for value in &chip.values {
                if all_selected {
                    set.remove(value);
                } else {
                    set.insert(value);
                }
            }
            // Mirror the chip toggle into the matching dropdown's rows.
            let set = ui.filter_set(chip.table, chip.kind);
            for (marker, values, mut dropdown) in &mut dropdowns {
                if marker.table != chip.table || marker.kind != chip.kind {
                    continue;
                }
                for (index, value) in values.0.iter().enumerate() {
                    if let Some(slot) = dropdown.selected.get_mut(index) {
                        *slot = set.contains(value);
                    }
                }
            }
        }
    }
}

fn open_buy_modal(
    resource: &str,
    vms: &ViewModels,
    theme: &Theme,
    ui: &mut TradeUi,
    commands: &mut Commands,
    modal_stack: &mut ModalStack,
) {
    let Some(trade) = vms.trade.as_ref() else {
        return;
    };
    // Web parity: the modal keys off the first offer with this resource.
    let Some(offer) = trade
        .available_offers
        .iter()
        .find(|o| o.resource == resource)
    else {
        return;
    };
    let max_qty = offer.quantity.min(trade.remaining_cargo).max(1);
    ui.buy = Some(BuyModalState {
        resource: resource.to_string(),
        price: offer.price,
        qty: 1,
    });
    let handles = widgets::open_modal(
        commands,
        modal_stack,
        theme,
        ModalProps {
            title: format!("Buy {}", split_camel(resource)),
            width: Val::Px(380.0),
        },
    );
    let price = offer.price;
    commands.entity(handles.content).with_children(|content| {
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("Quantity:"),
                    theme.font(13.0),
                    TextColor(theme::TEXT),
                ));
                let max = max_qty;
                let slider = widgets::spawn_slider(
                    row,
                    theme,
                    SliderProps {
                        min: 1.0,
                        max: max as f32,
                        step: 1.0,
                        value: 1.0,
                        width: Val::Px(170.0),
                        format: Some(Arc::new(move |v| format!("{v:.0} / {max}"))),
                        unlimited: false,
                    },
                );
                row.commands().entity(slider).insert(BuyQtySlider);
            });
        content.spawn((
            Text::new(format!("Market price: ${price} per unit")),
            theme.font(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        content
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                let cancel = widgets::spawn_button(row, theme, ButtonProps::label("Cancel"));
                row.commands().entity(cancel).insert(CancelBuyButton);
                let confirm = widgets::spawn_button(
                    row,
                    theme,
                    ButtonProps::label(format!("Buy 1 for ~${price}")),
                );
                row.commands().entity(confirm).insert(BuyConfirmButton);
            });
    });
}

pub fn handle_trade_sliders(
    mut commits: MessageReader<SliderCommitted>,
    sell: Query<&TradeSellSlider>,
    buy_qty: Query<(), With<BuyQtySlider>>,
    confirm_buttons: Query<&Children, With<BuyConfirmButton>>,
    mut labels: Query<&mut Text>,
    mut ui: ResMut<TradeUi>,
    mut out: MessageWriter<GameCommand>,
) {
    for commit in commits.read() {
        if let Ok(slider) = sell.get(commit.entity) {
            out.write(GameCommand::SetSellOrder {
                resource: slider.0.clone(),
                quantity: commit.as_u32(),
            });
        } else if buy_qty.contains(commit.entity) {
            let qty = commit.as_u32().max(1);
            let price = if let Some(buy) = ui.buy.as_mut() {
                buy.qty = qty;
                buy.price
            } else {
                continue;
            };
            for children in &confirm_buttons {
                for child in children {
                    if let Ok(mut text) = labels.get_mut(*child) {
                        **text = format!("Buy {qty} for ~${}", price * qty as i64);
                    }
                }
            }
        }
    }
}

pub fn handle_trade_filters(
    mut changes: MessageReader<MultiDropdownChanged>,
    dropdowns: Query<(&TradeFilterDropdown, &FilterValues)>,
    mut ui: ResMut<TradeUi>,
) {
    for change in changes.read() {
        let Ok((marker, values)) = dropdowns.get(change.entity) else {
            continue;
        };
        let selected = values
            .0
            .iter()
            .zip(change.selected.iter())
            .filter(|(_, on)| **on)
            .map(|(value, _)| value.clone());
        ui.filter_set(marker.table, marker.kind)
            .replace_from(selected);
    }
}

pub fn handle_hist_split(
    mut toggles: MessageReader<CheckboxToggled>,
    boxes: Query<(), With<HistSplitCheckbox>>,
    mut ui: ResMut<TradeUi>,
) {
    for toggle in toggles.read() {
        if boxes.contains(toggle.entity) {
            ui.hist_split = toggle.checked;
        }
    }
}
