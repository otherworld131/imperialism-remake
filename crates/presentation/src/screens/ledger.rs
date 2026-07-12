//! National Ledger (F7): full-screen overlay mirroring the web
//! `LedgerPanel` — nine tabs of per-Great-Power comparison tables with
//! turn-over-turn delta chips (previous-turn snapshot rotated only when the
//! turn advances), expandable rows, and runtime-rasterized nation flags.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use bevy::asset::RenderAssetUsages;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::game::resources::{PrevLedger, SessionRes, ViewModels};
use crate::game::vm::{self, GpLedgerEntryVm};
use crate::screens::common::{fmt_thousands, full_screen_root};
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ScrollProps, TooltipText};

const DELTA_GREEN: Color = Color::srgb_u8(0x2e, 0xcc, 0x40);
const DELTA_RED: Color = Color::srgb_u8(0xe6, 0x39, 0x46);
const GOODS_TEAL: Color = Color::srgb_u8(0x2a, 0x9d, 0x8f);
const CELL_GRAY: Color = Color::srgb_u8(0xbb, 0xbb, 0xbb);
const TRADE_BLUE: Color = Color::srgb_u8(0x66, 0xb3, 0xff);
const HUMAN_ROW_BG: Color = Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.04);
const EXPANDED_BG: Color = Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.03);

const TAB_LABELS: [&str; 9] = [
    "Economy",
    "Cash flow",
    "Expenses",
    "Production",
    "Resources",
    "Materials",
    "Military",
    "Diplomacy",
    "Technology",
];

const RESOURCE_ORDER: [&str; 12] = [
    "Timber",
    "Coal",
    "Iron",
    "Cotton",
    "Wool",
    "Grain",
    "Fruit",
    "Livestock",
    "Horses",
    "Oil",
    "Gold",
    "Gems",
];
const MATERIAL_ORDER: [&str; 6] = ["Lumber", "Steel", "Fabric", "Paper", "Arms", "CannedFood"];
const GOODS_ORDER: [&str; 3] = ["Furniture", "Clothing", "Hardware"];

fn material_label(name: &str) -> &str {
    if name == "CannedFood" {
        "Canned Food"
    } else {
        name
    }
}

// ── Flag cache ───────────────────────────────────────────────────────────

/// nation_id → rasterized flag image, invalidated when the SVG hash moves.
#[derive(Resource, Default)]
pub struct FlagCache {
    entries: HashMap<u32, FlagEntry>,
}

struct FlagEntry {
    svg_hash: u64,
    handle: Handle<Image>,
}

impl FlagCache {
    pub fn get(&self, nation_id: u32) -> Option<Handle<Image>> {
        self.entries.get(&nation_id).map(|e| e.handle.clone())
    }
}

const FLAG_W: u32 = 180;
const FLAG_H: u32 = 120;

fn hash_str(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Rasterize an SVG flag at 180×120 into a Bevy image.
fn rasterize_flag(svg: &str) -> Option<Image> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(FLAG_W, FLAG_H)?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return None;
    }
    let transform = tiny_skia::Transform::from_scale(
        FLAG_W as f32 / size.width(),
        FLAG_H as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia stores premultiplied alpha; demultiply for the GPU.
    let mut data = Vec::with_capacity((FLAG_W * FLAG_H * 4) as usize);
    for pixel in pixmap.pixels() {
        let demul = pixel.demultiply();
        data.extend_from_slice(&[demul.red(), demul.green(), demul.blue(), demul.alpha()]);
    }
    Some(Image::new(
        Extent3d {
            width: FLAG_W,
            height: FLAG_H,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ))
}

/// Build / refresh the flag cache when the ledger opens. The SVGs come from
/// the read-only `flavor::get_nation_flags` query; entries re-rasterize only
/// when an SVG's hash changes.
pub fn ensure_flags(
    session: Res<SessionRes>,
    mut cache: ResMut<FlagCache>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(session) = session.0.as_ref() else {
        return;
    };
    let flags = frontend_api::flavor::get_nation_flags(session.game());
    let Ok(flags) = vm::parse_nation_roster(flags) else {
        warn!("nation-flags decode failed");
        return;
    };
    for flag in flags {
        if flag.flag_svg.is_empty() {
            continue;
        }
        let svg_hash = hash_str(&flag.flag_svg);
        if cache
            .entries
            .get(&flag.nation_id)
            .is_some_and(|e| e.svg_hash == svg_hash)
        {
            continue;
        }
        let Some(image) = rasterize_flag(&flag.flag_svg) else {
            warn!("flag rasterization failed for nation {}", flag.nation_id);
            continue;
        };
        let handle = images.add(image);
        cache
            .entries
            .insert(flag.nation_id, FlagEntry { svg_hash, handle });
    }
}

// ── Screen state ─────────────────────────────────────────────────────────

/// Screen-local UI state: which nation row is expanded.
#[derive(Resource, Default)]
pub struct LedgerUi {
    pub expanded: Option<u32>,
}

#[derive(Component)]
pub struct LedgerRoot;

/// Content container of one tab (index into [`TAB_LABELS`]).
#[derive(Component)]
pub struct LedgerTabPanel(pub usize);

/// Whole-row expand toggle.
#[derive(Component)]
pub struct LedgerRowButton(pub u32);

#[derive(Component)]
pub struct LedgerCloseButton;

pub fn enter_ledger(mut commands: Commands, theme: Res<Theme>, mut ui: ResMut<LedgerUi>) {
    ui.expanded = None;
    let root = full_screen_root(&mut commands);
    commands.entity(root).insert(LedgerRoot);
    commands.entity(root).with_children(|panel| {
        // Header: title + close.
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::bottom(Val::Px(8.0)),
                    border: UiRect::bottom(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new("National Ledger"),
                    theme.font_bold(19.0),
                    TextColor(theme::GOLD),
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
                header.commands().entity(close).insert(LedgerCloseButton);
            });

        // Tabs; each panel hosts a scroll area whose content is rebuilt.
        let tabs = widgets::spawn_tabs(panel, &theme, &TAB_LABELS, 0);
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
            commands.entity(*tab_panel).with_children(|tab| {
                let scroll = widgets::spawn_scroll_area(
                    tab,
                    &theme,
                    ScrollProps {
                        flex_grow: 1.0,
                        ..default()
                    },
                );
                tab.commands()
                    .entity(scroll.content)
                    .insert(LedgerTabPanel(index));
            });
        }
    });
}

pub fn exit_ledger(mut commands: Commands, roots: Query<Entity, With<LedgerRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

pub fn handle_ledger_buttons(
    mut activations: MessageReader<ButtonActivated>,
    close_buttons: Query<(), With<LedgerCloseButton>>,
    mut next_screen: ResMut<NextState<crate::state::Screen>>,
) {
    for ButtonActivated(entity) in activations.read() {
        if close_buttons.contains(*entity) {
            next_screen.set(crate::state::Screen::Map);
        }
    }
}

/// Row clicks toggle the expanded nation (rows are plain `Interaction`
/// nodes, not kit buttons, so the human-row tint isn't restyled away).
pub fn handle_ledger_row_clicks(
    rows: Query<(&Interaction, &LedgerRowButton), Changed<Interaction>>,
    mut ui: ResMut<LedgerUi>,
) {
    for (interaction, row) in &rows {
        if *interaction == Interaction::Pressed {
            ui.expanded = if ui.expanded == Some(row.0) {
                None
            } else {
                Some(row.0)
            };
        }
    }
}

// ── Rebuild ──────────────────────────────────────────────────────────────

pub fn update_ledger(
    vms: Res<ViewModels>,
    prev: Res<PrevLedger>,
    ui: Res<LedgerUi>,
    flags: Res<FlagCache>,
    theme: Res<Theme>,
    mut commands: Commands,
    panels: Query<(Entity, &LedgerTabPanel)>,
    added: Query<(), Added<LedgerTabPanel>>,
) {
    if !vms.is_changed() && !ui.is_changed() && !flags.is_changed() && added.is_empty() {
        return;
    }
    if panels.is_empty() {
        return;
    }

    // Human first, then treasury descending (web sort).
    let mut sorted: Vec<&GpLedgerEntryVm> = vms.ledger.iter().collect();
    sorted.sort_by(|a, b| {
        b.is_human
            .cmp(&a.is_human)
            .then(b.economy.treasury.cmp(&a.economy.treasury))
    });
    let prev_map: HashMap<u32, &GpLedgerEntryVm> = if prev.entries.is_empty() {
        HashMap::new()
    } else {
        prev.entries.iter().map(|e| (e.nation_id, e)).collect()
    };

    let ctx = LedgerCtx {
        sorted: &sorted,
        prev: &prev_map,
        expanded: ui.expanded,
        flags: &flags,
        theme: &theme,
    };

    for (panel, tab) in &panels {
        commands.entity(panel).despawn_children();
        commands.entity(panel).with_children(|content| {
            match tab.0 {
                0 => economy_tab(content, &ctx),
                1 => cash_flow_tab(content, &ctx),
                2 => expenses_tab(content, &ctx),
                3 => production_tab(content, &ctx),
                4 => stockpile_tab(content, &ctx, StockpileTab::Resources),
                5 => stockpile_tab(content, &ctx, StockpileTab::Materials),
                6 => military_tab(content, &ctx),
                7 => diplomacy_tab(content, &ctx),
                _ => technology_tab(content, &ctx),
            };
        });
    }
}

struct LedgerCtx<'a> {
    sorted: &'a [&'a GpLedgerEntryVm],
    prev: &'a HashMap<u32, &'a GpLedgerEntryVm>,
    expanded: Option<u32>,
    flags: &'a FlagCache,
    theme: &'a Theme,
}

// ── Cell helpers ─────────────────────────────────────────────────────────

const NATION_COL: f32 = 170.0;

fn header_row(content: &mut ChildSpawnerCommands, theme: &Theme, columns: &[&str]) {
    content
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                padding: UiRect::vertical(Val::Px(6.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb_u8(0x33, 0x33, 0x33)),
        ))
        .with_children(|header| {
            header
                .spawn(Node {
                    flex_basis: Val::Px(NATION_COL),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|cell| {
                    cell.spawn((
                        Text::new("NATION"),
                        theme.font_bold(11.0),
                        TextColor(theme::GOLD),
                        Pickable::IGNORE,
                    ));
                });
            for column in columns {
                header
                    .spawn(Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        // Shrinkable below min-content: long headers wrap
                        // instead of pushing the row under the scrollbar.
                        min_width: Val::Px(0.0),
                        justify_content: JustifyContent::FlexEnd,
                        ..default()
                    })
                    .with_children(|cell| {
                        cell.spawn((
                            Text::new(column.to_uppercase()),
                            theme.font_bold(11.0),
                            TextColor(theme::GOLD),
                            Pickable::IGNORE,
                        ));
                    });
            }
        });
}

/// Clickable nation row: flag + name in the first column, then one
/// right-aligned cell per [`LedgerCell`].
fn nation_row(
    content: &mut ChildSpawnerCommands,
    ctx: &LedgerCtx,
    entry: &GpLedgerEntryVm,
    cells: Vec<LedgerCell>,
) {
    let theme = ctx.theme;
    let stripe = ctx
        .sorted
        .iter()
        .position(|e| e.nation_id == entry.nation_id)
        .is_some_and(|i| i % 2 == 1);
    let row = spawn_row_node(content, entry, stripe);
    let mut commands = content.commands();
    commands.entity(row).with_children(|cells_out| {
        nation_cell(cells_out, ctx, entry);
        for cell in cells {
            spawn_value_cell(cells_out, theme, cell, entry.is_human);
        }
    });
}

/// First column of every table: flag (or color dot) + name, human in gold.
fn nation_cell(row: &mut ChildSpawnerCommands, ctx: &LedgerCtx, entry: &GpLedgerEntryVm) {
    let theme = ctx.theme;
    row.spawn((
        Node {
            flex_basis: Val::Px(NATION_COL),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|cell| {
        match ctx.flags.get(entry.nation_id) {
            Some(image) => {
                cell.spawn((
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(16.0),
                        flex_shrink: 0.0,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.25)),
                    ImageNode::new(image),
                    Pickable::IGNORE,
                ));
            }
            None => {
                cell.spawn((
                    Node {
                        width: Val::Px(10.0),
                        height: Val::Px(10.0),
                        flex_shrink: 0.0,
                        border_radius: BorderRadius::all(Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(theme::nation_color(&entry.nation_color)),
                    Pickable::IGNORE,
                ));
            }
        }
        cell.spawn((
            Text::new(entry.nation_name.clone()),
            if entry.is_human {
                theme.font_bold(13.0)
            } else {
                theme.font(13.0)
            },
            TextColor(if entry.is_human {
                theme::GOLD
            } else {
                Color::srgb_u8(0xcc, 0xcc, 0xcc)
            }),
            Pickable::IGNORE,
        ));
    });
}

/// Clickable (expand/collapse) row node shared by every table; zebra
/// striping keeps wide rows scannable.
fn spawn_row_node(
    content: &mut ChildSpawnerCommands,
    entry: &GpLedgerEntryVm,
    stripe: bool,
) -> Entity {
    content
        .spawn((
            LedgerRowButton(entry.nation_id),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(0.0), Val::Px(6.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb_u8(0x1e, 0x1e, 0x30)),
            BackgroundColor(if entry.is_human {
                HUMAN_ROW_BG
            } else if stripe {
                Color::srgba(1.0, 1.0, 1.0, 0.025)
            } else {
                Color::NONE
            }),
            Interaction::default(),
        ))
        .id()
}

/// One numeric cell: formatted value plus a turn-over-turn delta chip.
struct LedgerCell {
    value: i64,
    prev: Option<i64>,
    money: bool,
    highlight: Option<Color>,
}

impl LedgerCell {
    fn count(value: i64, prev: Option<i64>) -> Self {
        Self {
            value,
            prev,
            money: false,
            highlight: None,
        }
    }

    fn money(value: i64, prev: Option<i64>) -> Self {
        Self {
            value,
            prev,
            money: true,
            highlight: None,
        }
    }

    fn highlight(mut self, when: bool, color: Color) -> Self {
        if when {
            self.highlight = Some(color);
        }
        self
    }
}

fn fmt_cell(value: i64, money: bool) -> String {
    if money {
        format!("${}", fmt_thousands(value))
    } else {
        fmt_thousands(value)
    }
}

fn spawn_value_cell(
    row: &mut ChildSpawnerCommands,
    theme: &Theme,
    cell: LedgerCell,
    is_human: bool,
) {
    row.spawn((
        Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            min_width: Val::Px(0.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|out| {
        let color = cell.highlight.unwrap_or(CELL_GRAY);
        out.spawn((
            Text::new(fmt_cell(cell.value, cell.money)),
            if cell.highlight.is_some() {
                theme.font_bold(12.5)
            } else {
                theme.font(12.5)
            },
            TextColor(color),
            Pickable::IGNORE,
        ));
        if let Some(prev) = cell.prev
            && prev != cell.value
        {
            let delta = cell.value - prev;
            let delta_str = if delta > 0 {
                format!("+{}", fmt_cell(delta, cell.money))
            } else {
                format!("-{}", fmt_cell(-delta, cell.money))
            };
            // CC-2: AI rows keep neutral deltas — red/green is reserved for
            // numbers the player owns and can act on.
            out.spawn((
                Text::new(delta_str),
                theme.font(10.0),
                TextColor(if !is_human {
                    CELL_GRAY
                } else if delta > 0 {
                    DELTA_GREEN
                } else {
                    DELTA_RED
                }),
                Pickable::IGNORE,
            ));
        }
    });
}

/// Full-width expanded panel under a nation row.
fn expanded_panel(
    content: &mut ChildSpawnerCommands,
    spawn: impl FnOnce(&mut ChildSpawnerCommands),
) {
    content
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::new(Val::Px(32.0), Val::Px(12.0), Val::Px(6.0), Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(EXPANDED_BG),
        ))
        .with_children(spawn);
}

fn detail_item(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    label: &str,
    value: &str,
    color: Color,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|item| {
            item.spawn((
                Text::new(format!("{label}:")),
                theme.font(12.5),
                TextColor(Color::srgb_u8(0x77, 0x77, 0x77)),
            ));
            item.spawn((Text::new(value), theme.font(12.5), TextColor(color)));
        });
}

// ── Tabs ─────────────────────────────────────────────────────────────────

fn economy_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    let theme = ctx.theme;
    header_row(
        content,
        theme,
        &[
            "Treasury",
            "Provinces",
            "Revenue",
            "Resources",
            "Materials",
            "Goods",
            "Workers",
        ],
    );
    for entry in ctx.sorted {
        let p = ctx.prev.get(&entry.nation_id);
        nation_row(
            content,
            ctx,
            entry,
            vec![
                LedgerCell::money(entry.economy.treasury, p.map(|p| p.economy.treasury)),
                LedgerCell::count(entry.economy.provinces, p.map(|p| p.economy.provinces)),
                LedgerCell::money(
                    entry.economy.goods_revenue,
                    p.map(|p| p.economy.goods_revenue),
                ),
                LedgerCell::count(
                    entry.economy.total_resources,
                    p.map(|p| p.economy.total_resources),
                ),
                LedgerCell::count(
                    entry.economy.total_materials,
                    p.map(|p| p.economy.total_materials),
                ),
                LedgerCell::count(entry.economy.total_goods, p.map(|p| p.economy.total_goods)),
                LedgerCell::count(entry.labor.total, p.map(|p| p.labor.total)),
            ],
        );
        if ctx.expanded == Some(entry.nation_id) {
            expanded_panel(content, |panel| {
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(20.0),
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    })
                    .with_children(|grid| {
                        detail_item(
                            grid,
                            theme,
                            "Untrained",
                            &entry.labor.untrained.to_string(),
                            Color::srgb_u8(0xcc, 0xcc, 0xcc),
                        );
                        detail_item(
                            grid,
                            theme,
                            "Trained",
                            &entry.labor.trained.to_string(),
                            Color::srgb_u8(0xcc, 0xcc, 0xcc),
                        );
                        detail_item(
                            grid,
                            theme,
                            "Expert",
                            &entry.labor.expert.to_string(),
                            Color::srgb_u8(0xcc, 0xcc, 0xcc),
                        );
                    });
                cash_category_breakdown(panel, theme, entry);
            });
        }
    }
}

/// "This turn's cash flow by category" inside the Economy expanded row.
fn cash_category_breakdown(
    panel: &mut ChildSpawnerCommands,
    theme: &Theme,
    entry: &GpLedgerEntryVm,
) {
    let Some(cf) = entry.cash_flow.as_ref() else {
        return;
    };
    let has_income = cf.income_by_category.values().any(|v| *v > 0);
    let has_expense = cf.expense_by_category.values().any(|v| *v > 0);
    if !has_income && !has_expense {
        return;
    }
    const CATEGORY_ORDER: [&str; 3] = ["Production", "Trade", "Consumption"];
    let category_color = |c: &str| match c {
        "Production" => DELTA_GREEN,
        "Trade" => TRADE_BLUE,
        "Consumption" => Color::srgb_u8(0xe6, 0x7e, 0x22),
        _ => Color::srgb_u8(0xcc, 0xcc, 0xcc),
    };
    panel.spawn((
        Text::new("THIS TURN'S CASH FLOW BY CATEGORY"),
        theme.font(11.5),
        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
    ));
    panel
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(24.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(|row| {
            for (label, color, map, has_any, sign) in [
                (
                    "Income:",
                    DELTA_GREEN,
                    &cf.income_by_category,
                    has_income,
                    "+",
                ),
                (
                    "Expense:",
                    DELTA_RED,
                    &cf.expense_by_category,
                    has_expense,
                    "−",
                ),
            ] {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|group| {
                    group.spawn((Text::new(label), theme.font_bold(13.5), TextColor(color)));
                    if !has_any {
                        group.spawn((
                            Text::new("(none)"),
                            theme.font(13.5),
                            TextColor(Color::srgb_u8(0x55, 0x55, 0x55)),
                        ));
                    }
                    for category in CATEGORY_ORDER {
                        let value = map.get(category).copied().unwrap_or(0);
                        if value == 0 {
                            continue;
                        }
                        group.spawn((
                            Text::new(format!("{category} {sign}${}", fmt_thousands(value))),
                            theme.font(13.5),
                            TextColor(category_color(category)),
                        ));
                    }
                });
            }
        });
}

fn cash_flow_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    let theme = ctx.theme;
    header_row(
        content,
        theme,
        &["Opening", "Closing", "Δ", "Income", "Expense", "Reconcile"],
    );
    for (index, entry) in ctx.sorted.iter().enumerate() {
        let cf = entry.cash_flow.as_ref();
        // Cash-flow cells render "—" before the first turn; build them as
        // plain rows (no delta chips, web parity).
        let row = spawn_row_node(content, entry, index % 2 == 1);
        let mut commands = content.commands();
        commands.entity(row).with_children(|cells| {
            nation_cell(cells, ctx, entry);
            let text_cell = |cells: &mut ChildSpawnerCommands, text: String, color: Color| {
                cells
                    .spawn((
                        Node {
                            flex_grow: 1.0,
                            flex_basis: Val::Px(0.0),
                            justify_content: JustifyContent::FlexEnd,
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|cell| {
                        cell.spawn((
                            Text::new(text),
                            theme.font(12.5),
                            TextColor(color),
                            Pickable::IGNORE,
                        ));
                    });
            };
            match cf {
                Some(cf) => {
                    text_cell(cells, fmt_money_signed(cf.opening_treasury), CELL_GRAY);
                    text_cell(cells, fmt_money_signed(cf.closing_treasury), CELL_GRAY);
                    text_cell(
                        cells,
                        fmt_money_signed(cf.observed_delta),
                        if cf.observed_delta >= 0 {
                            DELTA_GREEN
                        } else {
                            DELTA_RED
                        },
                    );
                    text_cell(cells, fmt_money_signed(cf.total_income), DELTA_GREEN);
                    text_cell(cells, fmt_money_signed(cf.total_expense), DELTA_RED);
                    if cf.reconciles {
                        text_cell(cells, "OK".into(), DELTA_GREEN);
                    } else {
                        text_cell(
                            cells,
                            format!("Δ {}", fmt_money_signed(cf.reconciliation_mismatch)),
                            DELTA_RED,
                        );
                    }
                }
                None => {
                    for _ in 0..6 {
                        text_cell(cells, "—".into(), Color::srgb_u8(0x88, 0x88, 0x88));
                    }
                }
            }
        });

        if ctx.expanded == Some(entry.nation_id)
            && let Some(cf) = cf
        {
            expanded_panel(content, |panel| {
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(28.0),
                        flex_wrap: FlexWrap::Wrap,
                        align_items: AlignItems::FlexStart,
                        ..default()
                    })
                    .with_children(|grid| {
                        cash_list(
                            grid,
                            theme,
                            &format!("Income (${})", fmt_thousands(cf.total_income)),
                            DELTA_GREEN,
                            cf.income_totals.iter().map(|(k, v)| (k.clone(), *v)),
                            "(no income this turn)",
                        );
                        cash_list(
                            grid,
                            theme,
                            &format!("Expense (${})", fmt_thousands(cf.total_expense)),
                            DELTA_RED,
                            cf.expense_totals.iter().map(|(k, v)| (k.clone(), *v)),
                            "(no expense this turn)",
                        );
                        // Cumulative.
                        grid.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            min_width: Val::Px(220.0),
                            ..default()
                        })
                        .with_children(|column| {
                            column.spawn((
                                Text::new("Cumulative (all turns)"),
                                theme.font_bold(12.5),
                                TextColor(theme::GOLD),
                            ));
                            for (k, v) in &entry.cumulative.income_totals {
                                detail_item(
                                    column,
                                    theme,
                                    &format!("+ {k}"),
                                    &fmt_money_signed(*v),
                                    DELTA_GREEN,
                                );
                            }
                            for (k, v) in &entry.cumulative.expense_totals {
                                detail_item(
                                    column,
                                    theme,
                                    &format!("− {k}"),
                                    &fmt_money_signed(*v),
                                    DELTA_RED,
                                );
                            }
                        });
                    });
            });
        }
    }
}

fn fmt_money_signed(n: i64) -> String {
    if n < 0 {
        format!("-${}", fmt_thousands(-n))
    } else {
        format!("${}", fmt_thousands(n))
    }
}

/// Sorted-descending label/amount list used by the cash-flow breakdowns.
fn cash_list(
    grid: &mut ChildSpawnerCommands,
    theme: &Theme,
    title: &str,
    color: Color,
    entries: impl Iterator<Item = (String, i64)>,
    empty: &str,
) {
    let mut entries: Vec<(String, i64)> = entries.collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    grid.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(2.0),
        min_width: Val::Px(220.0),
        ..default()
    })
    .with_children(|column| {
        column.spawn((Text::new(title), theme.font_bold(12.5), TextColor(color)));
        if entries.is_empty() {
            column.spawn((
                Text::new(empty),
                theme.font(12.0),
                TextColor(Color::srgb_u8(0x66, 0x66, 0x66)),
            ));
        }
        for (label, amount) in entries {
            detail_item(column, theme, &label, &fmt_money_signed(amount), color);
        }
    });
}

/// Expenses tab: the human player's full cash-flow reconciliation.
fn expenses_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    let theme = ctx.theme;
    let Some(human) = ctx
        .sorted
        .iter()
        .find(|e| e.is_human)
        .or_else(|| ctx.sorted.first())
    else {
        content.spawn((
            Text::new("No data available."),
            theme.font(12.5),
            TextColor(Color::srgb_u8(0x66, 0x66, 0x66)),
        ));
        return;
    };
    let cf = human.cash_flow.as_ref();

    content
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(40.0),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::FlexStart,
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        })
        .with_children(|columns| {
            cash_list(
                columns,
                theme,
                &match cf {
                    Some(cf) => format!("Income this turn: {}", fmt_money_signed(cf.total_income)),
                    None => "Income this turn".into(),
                },
                DELTA_GREEN,
                cf.map(|cf| cf.income_totals.clone())
                    .unwrap_or_default()
                    .into_iter(),
                "(no income this turn)",
            );
            cash_list(
                columns,
                theme,
                &match cf {
                    Some(cf) => {
                        format!("Expenses this turn: {}", fmt_money_signed(cf.total_expense))
                    }
                    None => "Expenses this turn".into(),
                },
                DELTA_RED,
                cf.map(|cf| cf.expense_totals.clone())
                    .unwrap_or_default()
                    .into_iter(),
                "(no expenses this turn)",
            );
            // Cumulative.
            columns
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    min_width: Val::Px(260.0),
                    ..default()
                })
                .with_children(|column| {
                    column.spawn((
                        Text::new("Cumulative (all turns)"),
                        theme.font_bold(14.0),
                        TextColor(theme::GOLD),
                    ));
                    let cumulative = &human.cumulative;
                    if cumulative.income_totals.is_empty() && cumulative.expense_totals.is_empty() {
                        column.spawn((
                            Text::new("(no history yet)"),
                            theme.font(12.0),
                            TextColor(Color::srgb_u8(0x55, 0x55, 0x55)),
                        ));
                    }
                    let mut income: Vec<_> = cumulative.income_totals.iter().collect();
                    income.sort_by(|a, b| b.1.cmp(a.1));
                    for (k, v) in income {
                        detail_item(
                            column,
                            theme,
                            &format!("+ {k}"),
                            &fmt_money_signed(*v),
                            DELTA_GREEN,
                        );
                    }
                    let mut expense: Vec<_> = cumulative.expense_totals.iter().collect();
                    expense.sort_by(|a, b| b.1.cmp(a.1));
                    for (k, v) in expense {
                        detail_item(
                            column,
                            theme,
                            &format!("− {k}"),
                            &fmt_money_signed(*v),
                            DELTA_RED,
                        );
                    }
                });
        });

    if let Some(cf) = cf {
        content
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    border: UiRect::top(Val::Px(1.0)),
                    padding: UiRect::all(Val::Px(8.0)),
                    margin: UiRect::top(Val::Px(12.0)),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                },
                BorderColor::all(Color::srgb_u8(0x22, 0x22, 0x22)),
            ))
            .with_children(|footer| {
                footer.spawn((
                    Text::new(format!(
                        "Treasury: {} → {} ({}{})",
                        fmt_money_signed(cf.opening_treasury),
                        fmt_money_signed(cf.closing_treasury),
                        if cf.observed_delta >= 0 { "+" } else { "" },
                        fmt_money_signed(cf.observed_delta),
                    )),
                    theme.font(12.5),
                    TextColor(Color::srgb_u8(0x77, 0x77, 0x77)),
                ));
                if !cf.reconciles {
                    footer.spawn((
                        Text::new(format!(
                            "⚠ reconciliation mismatch: {}",
                            fmt_money_signed(cf.reconciliation_mismatch)
                        )),
                        theme.font_bold(12.5),
                        TextColor(DELTA_RED),
                    ));
                }
            });
    }
}

fn production_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    header_row(
        content,
        ctx.theme,
        &[
            "Buildings",
            "Workers",
            "Untrained",
            "Trained",
            "Expert",
            "Revenue",
        ],
    );
    for entry in ctx.sorted {
        let p = ctx.prev.get(&entry.nation_id);
        nation_row(
            content,
            ctx,
            entry,
            vec![
                LedgerCell::count(entry.economy.buildings, p.map(|p| p.economy.buildings)),
                LedgerCell::count(entry.labor.total, p.map(|p| p.labor.total)),
                LedgerCell::count(entry.labor.untrained, p.map(|p| p.labor.untrained)),
                LedgerCell::count(entry.labor.trained, p.map(|p| p.labor.trained)),
                LedgerCell::count(entry.labor.expert, p.map(|p| p.labor.expert)),
                LedgerCell::money(
                    entry.economy.goods_revenue,
                    p.map(|p| p.economy.goods_revenue),
                ),
            ],
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StockpileTab {
    Resources,
    Materials,
}

fn stockpile_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx, kind: StockpileTab) {
    let theme = ctx.theme;
    let (columns, total_column): (Vec<String>, bool) = match kind {
        StockpileTab::Resources => (RESOURCE_ORDER.iter().map(|r| r.to_string()).collect(), true),
        StockpileTab::Materials => (
            MATERIAL_ORDER
                .iter()
                .chain(GOODS_ORDER.iter())
                .map(|m| material_label(m).to_string())
                .collect(),
            false,
        ),
    };
    let mut header: Vec<&str> = columns.iter().map(String::as_str).collect();
    if total_column {
        header.push("Total");
    }
    header_row(content, theme, &header);

    for entry in ctx.sorted {
        let p = ctx.prev.get(&entry.nation_id);
        let mut cells: Vec<LedgerCell> = Vec::new();
        match kind {
            StockpileTab::Resources => {
                for resource in RESOURCE_ORDER {
                    let cur = entry.resources_detail.get(resource).copied().unwrap_or(0);
                    let prv = p.map(|p| p.resources_detail.get(resource).copied().unwrap_or(0));
                    cells.push(LedgerCell::count(cur, prv).highlight(cur > 0, theme::GOLD));
                }
                let total = entry.economy.total_resources;
                cells.push(
                    LedgerCell::count(total, p.map(|p| p.economy.total_resources))
                        .highlight(true, theme::GOLD),
                );
            }
            StockpileTab::Materials => {
                for material in MATERIAL_ORDER {
                    let cur = entry.materials_detail.get(material).copied().unwrap_or(0);
                    let prv = p.map(|p| p.materials_detail.get(material).copied().unwrap_or(0));
                    cells.push(LedgerCell::count(cur, prv).highlight(cur > 0, theme::GOLD));
                }
                for good in GOODS_ORDER {
                    let cur = entry.goods_detail.get(good).copied().unwrap_or(0);
                    let prv = p.map(|p| p.goods_detail.get(good).copied().unwrap_or(0));
                    cells.push(LedgerCell::count(cur, prv).highlight(cur > 0, GOODS_TEAL));
                }
            }
        }
        nation_row(content, ctx, entry, cells);
        if ctx.expanded == Some(entry.nation_id) {
            let stockpiles: Vec<&str> = match kind {
                StockpileTab::Resources => RESOURCE_ORDER.to_vec(),
                StockpileTab::Materials => MATERIAL_ORDER
                    .iter()
                    .chain(GOODS_ORDER.iter())
                    .copied()
                    .collect(),
            };
            expanded_panel(content, |panel| {
                stockpile_flow_breakdown(panel, theme, entry, &stockpiles);
            });
        }
    }
}

/// Per-stockpile inflow/outflow by category with per-source tooltips
/// (web `StockpileCategoryBreakdown`).
fn stockpile_flow_breakdown(
    panel: &mut ChildSpawnerCommands,
    theme: &Theme,
    entry: &GpLedgerEntryVm,
    stockpiles: &[&str],
) {
    let Some(rf) = entry.resource_flow.as_ref() else {
        return;
    };

    // Group flat inflow/outflow entries by stockpile → category → source.
    type Detail = HashMap<String, HashMap<String, Vec<(String, i64)>>>;
    let mut inflow_detail: Detail = HashMap::new();
    for flow in &rf.inflow {
        let source = flow.source.clone().unwrap_or_else(|| "?".into());
        push_flow(
            inflow_detail
                .entry(flow.stockpile.clone())
                .or_default()
                .entry(flow.category.clone())
                .or_default(),
            source,
            flow.amount,
        );
    }
    let mut outflow_detail: Detail = HashMap::new();
    for flow in &rf.outflow {
        let sink = flow.sink.clone().unwrap_or_else(|| "?".into());
        push_flow(
            outflow_detail
                .entry(flow.stockpile.clone())
                .or_default()
                .entry(flow.category.clone())
                .or_default(),
            sink,
            flow.amount,
        );
    }

    let rows: Vec<&str> = stockpiles
        .iter()
        .copied()
        .filter(|stock| {
            rf.inflow_by_stockpile_category.contains_key(*stock)
                || rf.outflow_by_stockpile_category.contains_key(*stock)
                || inflow_detail.contains_key(*stock)
                || outflow_detail.contains_key(*stock)
        })
        .collect();
    if rows.is_empty() {
        panel.spawn((
            Text::new("No in/out movement this turn for these stockpiles."),
            theme.font(12.5),
            TextColor(Color::srgb_u8(0x66, 0x66, 0x66)),
        ));
        return;
    }

    panel.spawn((
        Text::new("THIS TURN'S FLOW BY CATEGORY — HOVER A VALUE TO SEE THE BREAKDOWN"),
        theme.font(10.5),
        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
    ));
    // Header.
    panel
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            padding: UiRect::vertical(Val::Px(4.0)),
            ..default()
        })
        .with_children(|header| {
            for (label, color, basis) in [
                ("Stockpile", theme::GOLD, 130.0),
                ("+ Production", DELTA_GREEN, 0.0),
                ("+ Trade", TRADE_BLUE, 0.0),
                ("− Consumption", DELTA_RED, 0.0),
                ("− Trade", TRADE_BLUE, 0.0),
            ] {
                header
                    .spawn(Node {
                        flex_basis: if basis > 0.0 {
                            Val::Px(basis)
                        } else {
                            Val::Px(0.0)
                        },
                        flex_grow: if basis > 0.0 { 0.0 } else { 1.0 },
                        flex_shrink: 0.0,
                        justify_content: if basis > 0.0 {
                            JustifyContent::FlexStart
                        } else {
                            JustifyContent::FlexEnd
                        },
                        ..default()
                    })
                    .with_children(|cell| {
                        cell.spawn((Text::new(label), theme.font_bold(11.0), TextColor(color)));
                    });
            }
        });
    for stock in rows {
        let in_cat = rf.inflow_by_stockpile_category.get(stock);
        let out_cat = rf.outflow_by_stockpile_category.get(stock);
        let in_detail = inflow_detail.get(stock);
        let out_detail = outflow_detail.get(stock);
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                padding: UiRect::vertical(Val::Px(2.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    flex_basis: Val::Px(130.0),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|cell| {
                    cell.spawn((
                        Text::new(material_label(stock)),
                        theme.font(12.5),
                        TextColor(theme::GOLD),
                    ));
                });
                for (category, agg, detail) in [
                    ("Production", in_cat, in_detail),
                    ("Trade", in_cat, in_detail),
                    ("Consumption", out_cat, out_detail),
                    ("Trade2", out_cat, out_detail),
                ] {
                    let category = if category == "Trade2" {
                        "Trade"
                    } else {
                        category
                    };
                    let sources = detail.and_then(|d| d.get(category));
                    let amount = agg
                        .and_then(|m| m.get(category).copied())
                        .or_else(|| sources.map(|s| s.iter().map(|(_, v)| *v).sum()));
                    flow_cell(row, theme, amount, sources);
                }
            });
    }
}

fn push_flow(list: &mut Vec<(String, i64)>, key: String, amount: i64) {
    match list.iter_mut().find(|(k, _)| *k == key) {
        Some((_, v)) => *v += amount,
        None => list.push((key, amount)),
    }
}

/// One flow cell: amount with a hover tooltip listing per-source amounts.
fn flow_cell(
    row: &mut ChildSpawnerCommands,
    theme: &Theme,
    amount: Option<i64>,
    sources: Option<&Vec<(String, i64)>>,
) {
    let display = match amount {
        None => "·".to_string(),
        Some(0) => "0".to_string(),
        Some(n) => fmt_thousands(n),
    };
    let tooltip = sources.and_then(|sources| {
        let mut entries: Vec<(String, i64)> =
            sources.iter().filter(|(_, v)| *v > 0).cloned().collect();
        if entries.is_empty() {
            return None;
        }
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        Some(
            entries
                .iter()
                .map(|(k, v)| format!("{k}: {}", fmt_thousands(*v)))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    });
    let cell = row
        .spawn(Node {
            flex_grow: 1.0,
            flex_basis: Val::Px(0.0),
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .with_children(|cell| {
            cell.spawn((Text::new(display), theme.font(12.5), TextColor(CELL_GRAY)));
        })
        .id();
    if let Some(tooltip) = tooltip {
        row.commands()
            .entity(cell)
            .insert((Interaction::default(), TooltipText(tooltip)));
    }
}

fn military_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    header_row(
        content,
        ctx.theme,
        &[
            "Field Army",
            "Militia",
            "Firepower",
            "Warships",
            "Merchants",
            "Arms Built",
            "Generals",
        ],
    );
    let max_fp = ctx
        .sorted
        .iter()
        .map(|e| e.military.total_army_fp)
        .max()
        .unwrap_or(0);
    for entry in ctx.sorted {
        let p = ctx.prev.get(&entry.nation_id);
        let m = &entry.military;
        nation_row(
            content,
            ctx,
            entry,
            vec![
                LedgerCell::count(m.field_army_count, p.map(|p| p.military.field_army_count)),
                LedgerCell::count(m.militia_count, p.map(|p| p.military.militia_count)),
                LedgerCell::count(m.total_army_fp, p.map(|p| p.military.total_army_fp))
                    .highlight(m.total_army_fp == max_fp, theme::GOLD),
                LedgerCell::count(
                    m.total_warship_count,
                    p.map(|p| p.military.total_warship_count),
                ),
                LedgerCell::count(m.merchant_ships, p.map(|p| p.military.merchant_ships)),
                LedgerCell::count(m.total_arms_built, p.map(|p| p.military.total_arms_built)),
                LedgerCell::count(m.generals_earned, p.map(|p| p.military.generals_earned)),
            ],
        );
    }
}

fn diplomacy_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    let theme = ctx.theme;
    header_row(
        content,
        theme,
        &["Standing", "Consulates", "Embassies", "Alliances", "Wars"],
    );
    for entry in ctx.sorted {
        let p = ctx.prev.get(&entry.nation_id);
        let d = &entry.diplomacy;
        nation_row(
            content,
            ctx,
            entry,
            vec![
                LedgerCell::count(d.standing, p.map(|p| p.diplomacy.standing)),
                LedgerCell::count(d.consulates, p.map(|p| p.diplomacy.consulates)),
                LedgerCell::count(d.embassies, p.map(|p| p.diplomacy.embassies)),
                LedgerCell::count(d.alliances, p.map(|p| p.diplomacy.alliances))
                    .highlight(d.alliances > 0, DELTA_GREEN),
                LedgerCell::count(d.wars, p.map(|p| p.diplomacy.wars))
                    .highlight(d.wars > 0, DELTA_RED),
            ],
        );
        if ctx.expanded == Some(entry.nation_id)
            && (!d.alliance_names.is_empty() || !d.war_names.is_empty())
        {
            expanded_panel(content, |panel| {
                if !d.alliance_names.is_empty() {
                    detail_item(
                        panel,
                        theme,
                        "Allied with",
                        &d.alliance_names.join(", "),
                        DELTA_GREEN,
                    );
                }
                if !d.war_names.is_empty() {
                    detail_item(
                        panel,
                        theme,
                        "At war with",
                        &d.war_names.join(", "),
                        DELTA_RED,
                    );
                }
            });
        }
    }
}

fn technology_tab(content: &mut ChildSpawnerCommands, ctx: &LedgerCtx) {
    let theme = ctx.theme;
    header_row(content, theme, &["Researched", "Technologies"]);
    for entry in ctx.sorted {
        let p = ctx.prev.get(&entry.nation_id);
        let current = &entry.technology.researched_names;
        let previous = p.map(|p| &p.technology.researched_names);
        let new_techs: Vec<&String> = match previous {
            Some(previous) if !previous.is_empty() => {
                current.iter().filter(|t| !previous.contains(t)).collect()
            }
            _ => Vec::new(),
        };

        let row = spawn_row_node(
            content,
            entry,
            ctx.sorted
                .iter()
                .position(|e| e.nation_id == entry.nation_id)
                .is_some_and(|i| i % 2 == 1),
        );
        let mut commands = content.commands();
        commands.entity(row).with_children(|cells| {
            nation_cell(cells, ctx, entry);
            // Count cell.
            cells
                .spawn((
                    Node {
                        flex_basis: Val::Px(110.0),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::FlexEnd,
                        column_gap: Val::Px(4.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|cell| {
                    cell.spawn((
                        Text::new(entry.technology.researched_count.to_string()),
                        theme.font(12.5),
                        TextColor(CELL_GRAY),
                        Pickable::IGNORE,
                    ));
                    if let Some(p) = p
                        && p.technology.researched_count != entry.technology.researched_count
                    {
                        let delta =
                            entry.technology.researched_count - p.technology.researched_count;
                        cell.spawn((
                            Text::new(format!("{delta:+}")),
                            theme.font(10.0),
                            TextColor(if delta > 0 { DELTA_GREEN } else { DELTA_RED }),
                            Pickable::IGNORE,
                        ));
                    }
                });
            // Tech-name list, new ones green.
            cells
                .spawn((
                    Node {
                        flex_grow: 1.0,
                        min_width: Val::Px(0.0),
                        margin: UiRect::left(Val::Px(16.0)),
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(4.0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|cell| {
                    if current.is_empty() {
                        cell.spawn((
                            Text::new("None"),
                            theme.font(12.0),
                            TextColor(Color::srgb_u8(0x99, 0x99, 0x99)),
                            Pickable::IGNORE,
                        ));
                    }
                    for (i, tech) in current.iter().enumerate() {
                        let is_new = new_techs.contains(&tech);
                        cell.spawn((
                            Text::new(if i + 1 < current.len() {
                                format!("{tech},")
                            } else {
                                tech.clone()
                            }),
                            theme.font(12.0),
                            TextColor(if is_new {
                                DELTA_GREEN
                            } else {
                                Color::srgb_u8(0x99, 0x99, 0x99)
                            }),
                            Pickable::IGNORE,
                        ));
                    }
                });
        });
    }
}
