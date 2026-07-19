//! Battle screen (F9, web `BattleScreen`): current-turn battles and the
//! per-turn battle archive. Land battles get an outcome banner, a political
//! province mini-map with red attack-origin arrows, terrain/fort context,
//! two-column per-unit force rosters, and medal awards; debug toggles reveal
//! the retreat math and the firepower walkthrough with the round-by-round
//! playout. Naval battles list fleets and deduped ship losses.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::game::resources::{
    CurrentTurnNews, DataVersion, NewsDebugSettings, SessionRes, TurnInfo, ViewModels,
};
use crate::game::vm::{self, ArchivedBattleTurnVm, BattleUnitLogVm, LandBattleVm, NavalBattleVm};
use crate::map::icons::IconAssets;
use crate::screens::common::{full_screen_root, split_camel, unit_icon_name};
use crate::screens::ledger::FlagCache;
use crate::screens::minimap::{self, HexCell, Raster, RasterParams};
use crate::screens::news::map_label;
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ScrollProps};

const SECTION_BG: Color = Color::srgb_u8(0x12, 0x12, 0x2a);
const WIN_GREEN: Color = Color::srgb_u8(0x2e, 0xcc, 0x40);
const LOSS_RED: Color = Color::srgb_u8(0xe6, 0x39, 0x46);
const SUB_GRAY: Color = Color::srgb_u8(0xaa, 0xaa, 0xaa);
const NUM_WHITE: Color = Color::srgb_u8(0xff, 0xff, 0xff);
const LABEL_BLUE: Color = Color::srgb_u8(0xaa, 0xaa, 0xbb);

// ── Screen state ─────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct BattlesUi {
    pub archive_mode: bool,
    pub selected_turn: Option<u32>,
    /// Index into land battles followed by naval battles.
    pub selected: usize,
}

/// Battle archive fetched from `get_battle_data`, refreshed when the data
/// version moves while the screen is open.
#[derive(Resource, Default)]
pub struct BattleArchive {
    pub turns: Vec<ArchivedBattleTurnVm>,
    pub version: u64,
}

#[derive(Component)]
pub struct BattlesRoot;

#[derive(Component)]
pub struct BattlesContent;

/// Marker on the Current/Archive tab-widget group (CC-5: same tab widget
/// as Trade/Ledger).
#[derive(Component)]
pub struct BattlesModeTabs;

#[derive(Component)]
pub struct BattlesArchiveTurnButton(pub u32);

#[derive(Component)]
pub struct BattlesRowButton(pub usize);

#[derive(Component)]
pub struct BattlesCloseButton;

pub fn enter_battles(mut commands: Commands, mut ui: ResMut<BattlesUi>) {
    ui.archive_mode = false;
    ui.selected_turn = None;
    ui.selected = 0;
    let root = full_screen_root(&mut commands);
    commands.entity(root).insert(BattlesRoot);
    commands.entity(root).with_children(|panel| {
        panel.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                ..default()
            },
            BattlesContent,
        ));
    });
}

pub fn exit_battles(mut commands: Commands, roots: Query<Entity, With<BattlesRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

/// Keep [`BattleArchive`] in step with the game state while the screen is
/// open (fetch on entry and after any end turn).
pub fn ensure_battle_archive(
    session: Res<SessionRes>,
    data_version: Res<DataVersion>,
    mut archive: ResMut<BattleArchive>,
) {
    if archive.version == data_version.0 {
        return;
    }
    let Some(session) = session.0.as_ref() else {
        return;
    };
    match frontend_api::battles::get_battle_data(session.game()).map(vm::parse_battle_archive) {
        Ok(Ok(turns)) => {
            archive.turns = turns;
            archive.version = data_version.0;
        }
        Ok(Err(err)) => warn!("battle-archive decode failed: {err}"),
        Err(err) => warn!("get_battle_data failed: {}", err.message()),
    }
}

// ── Interactions ─────────────────────────────────────────────────────────

pub fn handle_battles_tabs(
    mut changes: MessageReader<widgets::TabChanged>,
    tabs: Query<(), With<BattlesModeTabs>>,
    archive: Res<BattleArchive>,
    mut ui: ResMut<BattlesUi>,
) {
    for change in changes.read() {
        if !tabs.contains(change.group) {
            continue;
        }
        ui.archive_mode = change.index == 1;
        ui.selected = 0;
        if ui.archive_mode && ui.selected_turn.is_none() {
            // Auto-select the most recent archived turn (web parity).
            ui.selected_turn = archive.turns.iter().map(|t| t.turn).max();
        }
    }
}

pub fn handle_battles_buttons(
    mut activations: MessageReader<ButtonActivated>,
    archive_turns: Query<&BattlesArchiveTurnButton>,
    rows: Query<&BattlesRowButton>,
    closes: Query<(), With<BattlesCloseButton>>,
    mut ui: ResMut<BattlesUi>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(turn) = archive_turns.get(*entity) {
            ui.selected_turn = Some(turn.0);
            ui.selected = 0;
        } else if let Ok(row) = rows.get(*entity) {
            ui.selected = row.0;
        } else if closes.contains(*entity) {
            next_screen.set(Screen::Map);
        }
    }
}

// ── Rebuild ──────────────────────────────────────────────────────────────

pub fn update_battles(
    mut commands: Commands,
    theme: Res<Theme>,
    vms: Res<ViewModels>,
    news: Res<CurrentTurnNews>,
    archive: Res<BattleArchive>,
    flags: Res<FlagCache>,
    debug: Res<NewsDebugSettings>,
    icons: Option<Res<IconAssets>>,
    turn_info: Res<TurnInfo>,
    ui: Res<BattlesUi>,
    mut images: ResMut<Assets<Image>>,
    contents: Query<Entity, With<BattlesContent>>,
    added: Query<(), Added<BattlesContent>>,
) {
    if !ui.is_changed()
        && !news.is_changed()
        && !archive.is_changed()
        && !debug.is_changed()
        && !flags.is_changed()
        && added.is_empty()
    {
        return;
    }
    let Ok(content) = contents.single() else {
        return;
    };
    commands.entity(content).despawn_children();
    let icons = icons.as_deref();

    // Battles for the active mode.
    let archive_entry = ui
        .selected_turn
        .and_then(|turn| archive.turns.iter().find(|t| t.turn == turn));
    let (land, naval): (&[LandBattleVm], &[NavalBattleVm]) = if ui.archive_mode {
        archive_entry
            .map(|e| (e.battles.as_slice(), e.naval_battles.as_slice()))
            .unwrap_or((&[], &[]))
    } else {
        (news.battles.as_slice(), news.naval_battles.as_slice())
    };
    let total = land.len() + naval.len();
    let selected = if total > 0 {
        ui.selected.min(total - 1)
    } else {
        0
    };
    // Header date: the archived turn, the freshest report's turn, or the
    // live turn label before any turn has resolved.
    let turn_label = if ui.archive_mode && archive_entry.is_some() {
        archive_entry
            .map(|e| format!("{} Q{}", e.year, e.quarter))
            .unwrap_or_default()
    } else if news.has_report {
        format!("{} Q{}", news.year, news.quarter)
    } else {
        turn_info.label.clone()
    };

    commands.entity(content).with_children(|panel| {
        // ── Header ──
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(16.0),
                    padding: UiRect::bottom(Val::Px(8.0)),
                    border: UiRect::bottom(Val::Px(2.0)),
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new("Battles"),
                    theme.font_bold(19.0),
                    TextColor(theme::GOLD),
                ));
                header.spawn((
                    Text::new(turn_label.clone()),
                    theme.font(13.0),
                    TextColor(SUB_GRAY),
                ));
                header.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                let tabs = widgets::spawn_tabs(
                    header,
                    &theme,
                    &["Current", "Archive"],
                    usize::from(ui.archive_mode),
                );
                let mut header_commands = header.commands();
                header_commands.entity(tabs.root).insert((
                    BattlesModeTabs,
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                ));
                for panel in &tabs.panels {
                    header_commands.entity(*panel).insert(Node {
                        display: Display::None,
                        ..default()
                    });
                }
                let close = widgets::spawn_button(
                    header,
                    &theme,
                    ButtonProps {
                        label: "Close (Esc)".into(),
                        font_size: 12.0,
                        ..default()
                    },
                );
                header.commands().entity(close).insert(BattlesCloseButton);
            });

        // ── Body ──
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|body| {
                // Archive sidebar.
                if ui.archive_mode {
                    body.spawn((
                        Node {
                            width: Val::Px(150.0),
                            flex_shrink: 0.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            padding: UiRect::all(Val::Px(6.0)),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                        BackgroundColor(SECTION_BG),
                    ))
                    .with_children(|sidebar| {
                        sidebar.spawn((
                            Text::new("PAST TURNS"),
                            theme.font(11.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                        if archive.turns.is_empty() {
                            sidebar.spawn((
                                Text::new(
                                    "No battles in history yet — each End Turn's \
                                     battles are archived here.",
                                ),
                                theme.font_italic(12.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                        }
                        let mut turns: Vec<_> = archive.turns.iter().collect();
                        turns.sort_by(|a, b| b.turn.cmp(&a.turn));
                        for entry in turns {
                            let active = ui.selected_turn == Some(entry.turn);
                            let count = entry.battles.len() + entry.naval_battles.len();
                            let prefix = if active { "▶ " } else { "" };
                            let button = widgets::spawn_button(
                                sidebar,
                                &theme,
                                ButtonProps {
                                    label: format!(
                                        "{prefix}{} Q{}  ({count})",
                                        entry.year, entry.quarter
                                    ),
                                    font_size: 12.0,
                                    flat: true,
                                    ..default()
                                },
                            );
                            sidebar
                                .commands()
                                .entity(button)
                                .insert(BattlesArchiveTurnButton(entry.turn));
                        }
                    });
                }

                if total == 0 {
                    body.spawn(Node {
                        flex_grow: 1.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|empty| {
                        empty.spawn((
                            Text::new(if ui.archive_mode {
                                "Select a turn from the archive to view past battles."
                            } else {
                                "No battles occurred this turn."
                            }),
                            theme.font_italic(14.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                    });
                    return;
                }

                // Left column: mini-map + engagement list. Sized so the
                // details panel keeps readable width at 175% UI scale on a
                // 1280×720 window.
                body.spawn(Node {
                    width: Val::Px(MAP_W + 24.0),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|left| {
                    spawn_minimap(
                        left,
                        &theme,
                        &vms,
                        land.get(selected),
                        naval.get(selected.wrapping_sub(land.len())),
                        &mut images,
                    );
                    left.spawn((
                        Text::new("ENGAGEMENTS"),
                        theme.font(11.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                    left.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        overflow: Overflow::scroll_y(),
                        min_height: Val::Px(0.0),
                        ..default()
                    })
                    .with_children(|list| {
                        for (i, battle) in land.iter().enumerate() {
                            let winner = if battle.attacker_won {
                                (&battle.attacker, battle.attacker_id)
                            } else {
                                (&battle.defender, battle.defender_id)
                            };
                            battle_list_row(
                                list,
                                &theme,
                                &flags,
                                icons,
                                i,
                                selected == i,
                                "units",
                                "Army",
                                &format!("Battle of {}", battle.province),
                                winner,
                                battle.attacker_won,
                            );
                        }
                        for (i, battle) in naval.iter().enumerate() {
                            let index = land.len() + i;
                            let winner = if battle.attacker_won {
                                (&battle.attacker, battle.attacker_id)
                            } else {
                                (&battle.defender, battle.defender_id)
                            };
                            battle_list_row(
                                list,
                                &theme,
                                &flags,
                                icons,
                                index,
                                selected == index,
                                "ships",
                                "Frigate",
                                &format!("{} vs {}", battle.attacker, battle.defender),
                                winner,
                                battle.attacker_won,
                            );
                        }
                    });
                });

                // Right column: details. `width: Auto` (not the default
                // 100%) so the panel fills the space next to the fixed left
                // column instead of overflowing the right screen edge.
                let scroll = widgets::spawn_scroll_area(
                    body,
                    &theme,
                    ScrollProps {
                        width: Val::Auto,
                        flex_grow: 1.0,
                        ..default()
                    },
                );
                let mut body_commands = body.commands();
                body_commands
                    .entity(scroll.content)
                    .with_children(|details| {
                        if selected < land.len() {
                            land_battle_details(
                                details,
                                &theme,
                                &flags,
                                icons,
                                &debug,
                                &land[selected],
                            );
                        } else if let Some(battle) = naval.get(selected - land.len()) {
                            naval_battle_details(details, &theme, &flags, battle);
                        }
                    });
            });
    });
}

/// One engagement row in the battle list.
fn battle_list_row(
    list: &mut ChildSpawnerCommands,
    theme: &Theme,
    flags: &FlagCache,
    icons: Option<&IconAssets>,
    index: usize,
    active: bool,
    icon_group: &str,
    icon_name: &str,
    title: &str,
    winner: (&String, u32),
    attacker_won: bool,
) {
    let button = widgets::spawn_button(
        list,
        theme,
        ButtonProps {
            label: String::new(),
            font_size: 13.0,
            flat: !active,
            width: Some(Val::Percent(100.0)),
            ..default()
        },
    );
    let mut commands = list.commands();
    let mut entity = commands.entity(button);
    entity.insert(BattlesRowButton(index));
    entity.with_children(|row| {
        crate::screens::common::spawn_icon(row, icons, icon_group, icon_name, 16.0);
        row.spawn((
            Text::new(title.to_string()),
            theme.font(13.0),
            TextColor(if active { theme::GOLD } else { theme::TEXT }),
            Node {
                margin: UiRect::left(Val::Px(6.0)),
                ..default()
            },
        ));
        row.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        if let Some(flag) = flags.get(winner.1) {
            row.spawn((
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(11.0),
                    margin: UiRect::right(Val::Px(4.0)),
                    flex_shrink: 0.0,
                    ..default()
                },
                ImageNode::new(flag),
            ));
        }
        row.spawn((
            Text::new(format!("{} won", winner.0)),
            theme.font(12.0),
            TextColor(if attacker_won { WIN_GREEN } else { LOSS_RED }),
        ));
    });
}

// ── Province mini-map ────────────────────────────────────────────────────

const MAP_W: f32 = 300.0;
const MAP_H: f32 = 220.0;
const MAP_HEX: f32 = 15.0;
const MAP_RADIUS: i32 = 15;

fn spawn_minimap(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    vms: &ViewModels,
    land: Option<&LandBattleVm>,
    naval: Option<&NavalBattleVm>,
    images: &mut Assets<Image>,
) {
    let frame = Node {
        width: Val::Px(MAP_W),
        height: Val::Px(MAP_H),
        flex_shrink: 0.0,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        // Nation-label overlays near the edge must not spill out.
        overflow: Overflow::clip(),
        ..default()
    };

    let capital = land.and_then(|b| b.capital_tile);
    let (Some(battle), Some(capital), Some(tiles)) = (land, capital, vms.map.as_ref()) else {
        // Naval engagement / no land data: sea-blue placeholder (web parity).
        parent
            .spawn((
                frame,
                BackgroundColor(theme::terrain_color("Sea")),
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|placeholder| {
                placeholder
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|column| {
                        if let Some(battle) = naval {
                            column.spawn((
                                Text::new("Naval Engagement"),
                                theme.font_bold(16.0),
                                TextColor(theme::TEXT),
                            ));
                            column.spawn((
                                Text::new(format!("{} vs {}", battle.attacker, battle.defender)),
                                theme.font(12.0),
                                TextColor(theme::TEXT),
                            ));
                        } else {
                            column.spawn((
                                Text::new("No battle selected"),
                                theme.font_italic(13.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                        }
                    });
            });
        return;
    };

    // Tiles within the rendering radius, political fills.
    let center_px = minimap::hex_to_px(capital.q, capital.r, MAP_HEX);
    let offset = Vec2::new(MAP_W, MAP_H) / 2.0 - center_px;
    let mut country_ids: HashMap<i64, u32> = HashMap::new();
    let mut cells: HashMap<(i32, i32), HexCell> = HashMap::new();
    let mut label_tiles: Vec<(i32, i32, &str)> = Vec::new();
    for tile in tiles {
        let dq = tile.q - capital.q;
        let dr = tile.r - capital.r;
        if dq.abs().max(dr.abs()).max((dq + dr).abs()) > MAP_RADIUS {
            continue;
        }
        let fill = if tile.is_sea() {
            minimap::rgb(theme::terrain_color("Sea"))
        } else if tile.owner_color.is_empty() {
            minimap::rgb(theme::terrain_color(&tile.terrain))
        } else {
            let nation = theme::nation_color(&tile.owner_color);
            minimap::rgb(if tile.is_incorporated_minor {
                theme::incorporated_tint(nation)
            } else {
                theme::political_tint(nation)
            })
        };
        let country = if !tile.is_sea() && !tile.owner.is_empty() {
            let next = country_ids.len() as u32;
            *country_ids.entry(tile.nation_id).or_insert(next)
        } else {
            u32::MAX
        };
        if !tile.is_sea() && !tile.owner.is_empty() {
            label_tiles.push((tile.q, tile.r, tile.visual_group_or_owner()));
        }
        cells.insert(
            (tile.q, tile.r),
            HexCell {
                fill,
                country,
                province: 0,
            },
        );
    }

    let mut raster = Raster::new(
        RasterParams {
            width: MAP_W as u32,
            height: MAP_H as u32,
            hex_size: MAP_HEX,
            offset,
            background: minimap::rgb(Color::srgb_u8(0x0a, 0x0a, 0x1e)),
        },
        &cells,
    );
    // Battle-province highlight.
    let province: HashSet<(i32, i32)> = battle.province_tiles.iter().map(|t| (t.q, t.r)).collect();
    raster.tint_hexes(&province, [255, 80, 40], 0.35);
    // Attack-origin arrows.
    let to = minimap::hex_to_px(capital.q, capital.r, MAP_HEX) + offset;
    for origin in &battle.origin_tiles {
        let from = minimap::hex_to_px(origin.q, origin.r, MAP_HEX) + offset;
        raster.draw_arrow(from, to, [255, 51, 51], 2.5);
    }
    // Battle capital marker (the web draws ⚔; a ringed red dot reads at
    // this scale without relying on emoji glyph coverage).
    raster.draw_dot(to, 4.0, [255, 64, 64], [26, 26, 26]);
    let image = images.add(raster.into_image());

    let labels = minimap::place_nation_labels(
        minimap::compute_nation_labels(&label_tiles, 5, MAP_HEX, offset),
        Vec2::new(MAP_W, MAP_H),
        3.0,
        10.0,
        18.0,
    );
    parent
        .spawn((
            frame,
            ImageNode::new(image),
            BorderColor::all(theme::BORDER),
        ))
        .with_children(|map| {
            for label in &labels {
                map_label(map, theme, &label.name, label.pos, label.font_size);
            }
        });
}

// ── Land battle details ──────────────────────────────────────────────────

fn land_battle_details(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    flags: &FlagCache,
    icons: Option<&IconAssets>,
    debug: &NewsDebugSettings,
    battle: &LandBattleVm,
) {
    let (winner_name, winner_id) = if battle.attacker_won {
        (&battle.attacker, battle.attacker_id)
    } else {
        (&battle.defender, battle.defender_id)
    };

    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|details| {
            // Outcome banner.
            details
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::axes(Val::Px(20.0), Val::Px(12.0)),
                        border: UiRect::left(Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(SECTION_BG),
                    BorderColor::all(if battle.attacker_won {
                        WIN_GREEN
                    } else {
                        LOSS_RED
                    }),
                ))
                .with_children(|banner| {
                    banner
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|title| {
                            if let Some(flag) = flags.get(winner_id) {
                                title.spawn((
                                    Node {
                                        width: Val::Px(24.0),
                                        height: Val::Px(16.0),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                    ImageNode::new(flag),
                                ));
                            }
                            title.spawn((
                                Text::new(format!("{winner_name} Victory")),
                                theme.font_bold(19.0),
                                TextColor(theme::GOLD),
                            ));
                        });
                    banner.spawn((
                        Text::new(format!(
                            "at {}{}",
                            battle.province,
                            if battle.retreated {
                                " (attacker retreated)"
                            } else {
                                ""
                            }
                        )),
                        theme.font(13.0),
                        TextColor(SUB_GRAY),
                    ));
                    if battle.is_naval_landing || !battle.origin_province_names.is_empty() {
                        banner
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                flex_wrap: FlexWrap::Wrap,
                                ..default()
                            })
                            .with_children(|line| {
                                if battle.is_naval_landing {
                                    line.spawn((
                                        Node {
                                            padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                                            border: UiRect::all(Val::Px(1.0)),
                                            border_radius: BorderRadius::all(Val::Px(3.0)),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(
                                            218.0 / 255.0,
                                            165.0 / 255.0,
                                            32.0 / 255.0,
                                            0.15,
                                        )),
                                        BorderColor::all(theme::GOLD),
                                    ))
                                    .with_children(|badge| {
                                        badge.spawn((
                                            Text::new("Naval Landing"),
                                            theme.font(11.0),
                                            TextColor(theme::GOLD),
                                        ));
                                    });
                                }
                                if !battle.origin_province_names.is_empty() {
                                    line.spawn((
                                        Text::new(format!(
                                            "Origin: {}",
                                            battle.origin_province_names.join(", ")
                                        )),
                                        theme.font(12.0),
                                        TextColor(Color::srgb_u8(0xd8, 0xd0, 0xb8)),
                                    ));
                                }
                            });
                    }
                });

            // Battlefield (terrain + fort) — the setting reads first.
            section(details, theme, "BATTLEFIELD", |body, theme| {
                body.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(24.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|row| {
                    if let Some(terrain) = battle.terrain.as_deref() {
                        row.spawn((
                            Text::new(format!("Terrain: {terrain}")),
                            theme.font(13.0),
                            TextColor(Color::srgb_u8(0xbb, 0xbb, 0xbb)),
                        ));
                    }
                    let siege = if battle.siege_reduced_fort {
                        " (reduced by siege)"
                    } else {
                        ""
                    };
                    row.spawn((
                        Text::new(format!("Fort Level: {}{siege}", battle.fort_level)),
                        theme.font(13.0),
                        TextColor(if battle.siege_reduced_fort {
                            theme::GOLD
                        } else {
                            Color::srgb_u8(0xbb, 0xbb, 0xbb)
                        }),
                    ));
                });
            });

            // Retreat math (debug).
            if debug.show_retreat_debug && battle.retreat_debug.is_some() {
                section(details, theme, "RETREAT MATH (DEBUG)", |body, theme| {
                    retreat_debug_block(body, theme, battle);
                });
            }

            // Forces.
            section(details, theme, "FORCES", |body, theme| {
                // One-line legend for the per-unit indicators (CC-4).
                body.spawn((
                    Text::new("bar = remaining strength · ★ = medals (veterancy)"),
                    theme.font_italic(10.5),
                    TextColor(theme::TEXT_DIM),
                    Node {
                        margin: UiRect::bottom(Val::Px(4.0)),
                        ..default()
                    },
                ));
                body.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(24.0),
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|grid| {
                    force_column(grid, theme, flags, icons, debug, battle, true);
                    force_column(grid, theme, flags, icons, debug, battle, false);
                });
            });

            // Firepower walkthrough + round playout (debug).
            if debug.show_battle_firepower {
                section(details, theme, "HOW COMBAT IS CALCULATED", |body, theme| {
                    combat_explanation(body, theme, battle);
                });
                if !battle.round_logs.is_empty() {
                    section(
                        details,
                        theme,
                        "HOW THE BATTLE PLAYED OUT",
                        |body, theme| {
                            round_playout(body, theme, battle);
                        },
                    );
                }
            }

            // Medal awards.
            if !battle.medal_awards.is_empty() {
                section(details, theme, "MEDALS AWARDED", |body, theme| {
                    for medal in &battle.medal_awards {
                        body.spawn((
                            Text::new(format!(
                                "★ {} — {} medal{}",
                                split_camel(&medal.unit_type),
                                medal.medals,
                                if medal.medals > 1 { "s" } else { "" }
                            )),
                            theme.font(13.0),
                            TextColor(theme::GOLD),
                        ));
                    }
                });
            }
        });
}

/// Dark inset card with an uppercase section title.
fn section(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    title: &str,
    body: impl FnOnce(&mut ChildSpawnerCommands, &Theme),
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(SECTION_BG),
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    border: UiRect::bottom(Val::Px(1.0)),
                    padding: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb_u8(0x2a, 0x2a, 0x3e)),
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new(title.to_string()),
                    theme.font(11.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
            body(card, theme);
        });
}

// ── Force column with per-unit rows ─────────────────────────────────────

/// Web `UNIT_TYPE_CATEGORY` (icon + attacker-modifier hints).
fn unit_category(unit_type: &str) -> &'static str {
    match unit_type {
        "Minutemen" | "Militia" | "Conscript" | "GarrisonArtillery" => "Garrison",
        "Skirmishers" | "Sharpshooters" | "Rangers" | "Regulars" | "RifleInfantry" | "Infantry"
        | "Grenadiers" | "Guards" | "MachineGunners" => "Infantry",
        "Hussars" | "Scouts" | "Carbineers" | "Mechanised" | "Cuirassiers" | "Armour" => "Cavalry",
        "LightArtillery" | "HorseArtillery" | "FieldArtillery" | "MobileArtillery"
        | "Artillery" | "SiegeArtillery" | "RailroadGuns" => "Artillery",
        _ => "Special",
    }
}

/// Web `UNIT_RANGE` (mirrors `scripts/config/units.lua`; display only).
fn unit_range(unit_type: &str) -> u32 {
    match unit_type {
        "Minutemen" | "Skirmishers" | "Regulars" | "Grenadiers" | "Hussars" | "Scouts"
        | "Cuirassiers" | "Sapper" | "Saboteur" => 1,
        "Militia" | "Conscript" | "RifleInfantry" | "Infantry" | "Guards" | "MachineGunners"
        | "Carbineers" | "CombatEngineer" | "Commandos" => 2,
        "GarrisonArtillery" | "Sharpshooters" | "LightArtillery" => 3,
        "Mechanised" | "HorseArtillery" | "Artillery" => 4,
        "Rangers" | "FieldArtillery" | "MobileArtillery" => 5,
        "Armour" | "SiegeArtillery" => 6,
        "RailroadGuns" => 17,
        _ => 0,
    }
}

fn health_color(pct: f32) -> Color {
    if pct > 60.0 {
        Color::srgb_u8(0x2a, 0xa2, 0x2a)
    } else if pct > 30.0 {
        Color::srgb_u8(0xca, 0xa2, 0x20)
    } else {
        Color::srgb_u8(0xa2, 0x22, 0x22)
    }
}

struct UnitRowSpec<'a> {
    unit_type: &'a str,
    medals: u32,
    health: u32,
    /// `(initial, final)` when the firepower toggle is on, else just final.
    fp: Option<(Option<f64>, f64)>,
    destroyed: bool,
    /// Extra annotation under the FP line.
    suffix: Option<String>,
}

/// Port of the web `UnitRow`: icon + name + medal stars + FP, HP bar below.
/// Destroyed units render dimmed with a ✕ prefix (Bevy text has no
/// strikethrough).
fn unit_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    spec: UnitRowSpec,
) {
    let dim = spec.destroyed;
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                margin: UiRect::bottom(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, if dim { 0.02 } else { 0.05 })),
        ))
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|line| {
                crate::screens::common::spawn_icon(
                    line,
                    icons,
                    "units",
                    unit_icon_name(unit_category(spec.unit_type)),
                    14.0,
                );
                let name = if dim {
                    format!("✕ {}", split_camel(spec.unit_type))
                } else {
                    split_camel(spec.unit_type)
                };
                line.spawn((
                    Text::new(name),
                    theme.font(12.5),
                    TextColor(if dim { theme::TEXT_DIM } else { theme::TEXT }),
                ));
                if !dim && spec.medals > 0 {
                    line.spawn((
                        Text::new("★".repeat(spec.medals as usize)),
                        theme.font(12.0),
                        TextColor(Color::srgb_u8(0xff, 0xd7, 0x00)),
                    ));
                }
                line.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                match spec.fp {
                    Some((Some(initial), final_fp)) => {
                        line.spawn((
                            Text::new(format!("FP {initial:.1} → {final_fp:.1}")),
                            theme.font(11.0),
                            TextColor(if dim {
                                Color::srgb_u8(0xa6, 0x66, 0x66)
                            } else {
                                Color::srgb_u8(0x99, 0x99, 0x99)
                            }),
                        ));
                    }
                    Some((None, final_fp)) if !dim => {
                        line.spawn((
                            Text::new(format!("FP {final_fp:.1}")),
                            theme.font(11.0),
                            TextColor(Color::srgb_u8(0x99, 0x99, 0x99)),
                        ));
                    }
                    _ => {
                        if dim {
                            line.spawn((
                                Text::new("Destroyed"),
                                theme.font(10.0),
                                TextColor(Color::srgb_u8(0xa6, 0x66, 0x66)),
                            ));
                        }
                    }
                }
            });
            if !dim {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|line| {
                    let pct = spec.health as f32;
                    line.spawn((
                        Node {
                            width: Val::Px(60.0),
                            height: Val::Px(5.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
                    ))
                    .with_children(|track| {
                        track.spawn((
                            Node {
                                width: Val::Percent(pct.clamp(0.0, 100.0)),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(health_color(pct)),
                        ));
                    });
                    line.spawn((
                        Text::new(format!("{}%", spec.health)),
                        theme.font(10.0),
                        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
                    ));
                });
            }
            if let Some(suffix) = spec.suffix {
                row.spawn((Text::new(suffix), theme.font(10.0), TextColor(LABEL_BLUE)));
            }
        });
}

fn force_column(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    flags: &FlagCache,
    icons: Option<&IconAssets>,
    debug: &NewsDebugSettings,
    battle: &LandBattleVm,
    attacker: bool,
) {
    let (side, side_id, role, initial, survived, survivors, casualties, logs) = if attacker {
        (
            &battle.attacker,
            battle.attacker_id,
            "Attacker",
            battle.attacker_initial_count,
            battle.attacker_survivors_count,
            &battle.attacker_survivors,
            &battle.attacker_casualties,
            &battle.attacker_unit_logs,
        )
    } else {
        (
            &battle.defender,
            battle.defender_id,
            "Defender",
            battle.defender_initial_count,
            battle.defender_survivors_count,
            &battle.defender_survivors,
            &battle.defender_casualties,
            &battle.defender_unit_logs,
        )
    };
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_basis: Val::Percent(50.0),
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            ..default()
        })
        .with_children(|column| {
            column
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    // Wrap "(Attacker)" under the name in narrow columns
                    // instead of running into the neighbouring header.
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|header| {
                    if let Some(flag) = flags.get(side_id) {
                        header.spawn((
                            Node {
                                width: Val::Px(20.0),
                                height: Val::Px(13.0),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            ImageNode::new(flag),
                        ));
                    }
                    header.spawn((
                        Text::new(side.clone()),
                        theme.font_bold(13.0),
                        TextColor(theme::TEXT),
                    ));
                    header.spawn((
                        Text::new(format!("({role})")),
                        theme.font(11.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                });
            // One text node per stat so a narrow column wraps between the
            // stats ("… · 0 lost") instead of mid-phrase ("0\nlost").
            column
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(4.0),
                    margin: UiRect::bottom(Val::Px(4.0)),
                    ..default()
                })
                .with_children(|stats| {
                    for (i, stat) in [
                        format!("{initial} engaged"),
                        format!("{survived} survived"),
                        format!("{} lost", casualties.len()),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let text = if i == 0 { stat } else { format!("· {stat}") };
                        stats.spawn((
                            Text::new(text),
                            theme.font(12.0),
                            TextColor(Color::srgb_u8(0xbb, 0xbb, 0xbb)),
                        ));
                    }
                });

            let use_logs = debug.show_battle_firepower && !logs.is_empty();
            if use_logs {
                for log in logs {
                    let destroyed = log.final_health == 0;
                    let suffix = if let Some(breakdown) = &log.defender_breakdown {
                        Some(format!(
                            "Defender contrib: {:.2} = fp {:.2} × fort {:.2}{}",
                            breakdown.initial_total_contribution,
                            breakdown.applied_firepower,
                            breakdown.fort_multiplier,
                            if breakdown.entrenchment_fp > 0.0 {
                                format!(" + entrenchment {:.2}", breakdown.entrenchment_fp)
                            } else {
                                String::new()
                            }
                        ))
                    } else if attacker && unit_category(&log.unit_type) == "Cavalry" {
                        Some("applied: round-1 FPM × 1.25 charge".to_string())
                    } else {
                        None
                    };
                    unit_row(
                        column,
                        theme,
                        icons,
                        UnitRowSpec {
                            unit_type: &log.unit_type,
                            medals: if destroyed {
                                log.medals_initial
                            } else {
                                log.medals_final
                            },
                            health: log.final_health,
                            fp: Some((Some(log.initial_firepower), log.final_firepower)),
                            destroyed,
                            suffix,
                        },
                    );
                }
            } else {
                for unit in survivors {
                    unit_row(
                        column,
                        theme,
                        icons,
                        UnitRowSpec {
                            unit_type: &unit.unit_type,
                            medals: unit.medals,
                            health: unit.health,
                            fp: debug
                                .show_battle_firepower
                                .then_some((None, unit.effective_firepower)),
                            destroyed: false,
                            suffix: None,
                        },
                    );
                }
                for unit_type in casualties {
                    unit_row(
                        column,
                        theme,
                        icons,
                        UnitRowSpec {
                            unit_type,
                            medals: 0,
                            health: 0,
                            fp: None,
                            destroyed: true,
                            suffix: None,
                        },
                    );
                }
            }
            if survivors.is_empty() && casualties.is_empty() && !use_logs {
                column.spawn((
                    Text::new("No units recorded"),
                    theme.font_italic(12.0),
                    TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
                ));
            }
        });
}

// ── Retreat math (debug) ─────────────────────────────────────────────────

/// `{:.2}` for finite values, `∞` / `NaN` otherwise (web `fmt` falls back
/// to `String(n)` for non-finite ratios).
fn fmt2(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.2}")
    } else if value.is_nan() {
        "NaN".to_string()
    } else {
        "∞".to_string()
    }
}

fn retreat_debug_block(parent: &mut ChildSpawnerCommands, theme: &Theme, battle: &LandBattleVm) {
    let Some(debug) = battle.retreat_debug.as_ref() else {
        return;
    };
    let side = if debug.side == "attacker" {
        "Attacker"
    } else {
        "Defender"
    };
    let summary = match debug.stage.as_str() {
        "pre_battle" => format!(
            "{side} bailed pre-battle. (opposing FP / own FP) = {} > threshold {}",
            fmt2(debug.measured_value),
            fmt2(debug.threshold)
        ),
        "mid_battle" => format!(
            "{side} retreated mid-battle (round {}). FP loss = {:.0}% > threshold {:.0}%",
            debug.round,
            debug.measured_value * 100.0,
            debug.threshold * 100.0
        ),
        _ => format!(
            "No retreat fired. Battle resolved over {} round(s).",
            debug.round
        ),
    };
    parent.spawn((
        Text::new(summary),
        theme.font(12.0),
        TextColor(Color::srgb_u8(0xdc, 0xd6, 0xc4)),
    ));
    parent.spawn((
        Text::new(format!(
            "Pre-battle ratios: atk {} (thr {}) · def {} (thr {})",
            fmt2(debug.attacker_prebattle_ratio),
            fmt2(debug.attacker_prebattle_threshold),
            fmt2(debug.defender_prebattle_ratio),
            fmt2(debug.defender_prebattle_threshold)
        )),
        theme.font(11.0),
        TextColor(LABEL_BLUE),
    ));
    parent.spawn((
        Text::new(format!(
            "Effective FP: atk {:.2} · def {:.2}",
            battle.attacker_initial_fp, battle.defender_initial_fp
        )),
        theme.font(11.0),
        TextColor(LABEL_BLUE),
    ));
    parent.spawn((
        Text::new(format!(
            "Includes per-unit DEF stat × (1 + terrain{}) × (1 + fort L{}) × general bonus, \
             + 8 FP per defending militia (entrenchment). That's why this is much larger than \
             the sum of unit FP shown above.",
            battle
                .terrain
                .as_deref()
                .map(|t| format!(" {t}"))
                .unwrap_or_default(),
            battle.fort_level
        )),
        theme.font_italic(10.5),
        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
    ));
}

// ── Firepower walkthrough (debug) ────────────────────────────────────────

fn combat_explanation(parent: &mut ChildSpawnerCommands, theme: &Theme, battle: &LandBattleVm) {
    let atk_sum: f64 = battle
        .attacker_unit_logs
        .iter()
        .map(|u| u.initial_firepower)
        .sum();
    let def_contrib = |u: &BattleUnitLogVm| {
        u.defender_breakdown
            .as_ref()
            .map(|b| b.initial_total_contribution)
            .unwrap_or(u.initial_firepower)
    };
    let def_sum: f64 = battle.defender_unit_logs.iter().map(def_contrib).sum();
    let atk_bonus = if atk_sum > 0.0 {
        battle.attacker_initial_fp / atk_sum
    } else {
        1.0
    };
    let def_bonus = if def_sum > 0.0 {
        battle.defender_initial_fp / def_sum
    } else {
        1.0
    };

    let walkthrough = |parent: &mut ChildSpawnerCommands,
                       theme: &Theme,
                       heading: String,
                       note: &str,
                       rows: Vec<(String, String)>,
                       sum: f64,
                       bonus: f64,
                       total: f64| {
        parent.spawn((
            Text::new(heading),
            theme.font_bold(12.5),
            TextColor(NUM_WHITE),
        ));
        parent.spawn((
            Text::new(note.to_string()),
            theme.font_italic(11.0),
            TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
        ));
        for (label, value) in rows {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: Val::Px(150.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        Text::new(label),
                        theme.font(11.0),
                        TextColor(LABEL_BLUE),
                    ));
                    row.spawn((Text::new(value), theme.font(11.0), TextColor(NUM_WHITE)));
                });
        }
        parent
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(12.0),
                margin: UiRect::bottom(Val::Px(8.0)),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Node {
                        width: Val::Px(150.0),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    Text::new("Sum × general bonus:"),
                    theme.font(11.0),
                    TextColor(LABEL_BLUE),
                ));
                row.spawn((
                    Text::new(format!("{sum:.2} × {bonus:.2} = {total:.2}")),
                    theme.font(11.0),
                    TextColor(NUM_WHITE),
                ));
            });
    };

    walkthrough(
        parent,
        theme,
        format!("Attacker initial FP: {:.2}", battle.attacker_initial_fp),
        "Each unit's contribution is its applied firepower (FPN × medals × health, plus FPM \
         swap and ×1.25 charge for round-1 cavalry).",
        battle
            .attacker_unit_logs
            .iter()
            .map(|u| {
                (
                    format!("{}:", split_camel(&u.unit_type)),
                    format!("{:.2}", u.initial_firepower),
                )
            })
            .collect(),
        atk_sum,
        atk_bonus,
        battle.attacker_initial_fp,
    );
    walkthrough(
        parent,
        theme,
        format!("Defender initial FP: {:.2}", battle.defender_initial_fp),
        "Each unit's contribution = applied_fp × fort + entrenchment (Garrison units in the \
         province for ≥1 turn).",
        battle
            .defender_unit_logs
            .iter()
            .map(|u| {
                let contrib = def_contrib(u);
                let detail = u
                    .defender_breakdown
                    .as_ref()
                    .map(|b| {
                        format!(
                            " ({:.2} × {:.2}{})",
                            b.applied_firepower,
                            b.fort_multiplier,
                            if b.entrenchment_fp > 0.0 {
                                format!(" + {:.2}", b.entrenchment_fp)
                            } else {
                                String::new()
                            }
                        )
                    })
                    .unwrap_or_default();
                (
                    format!("{}:", split_camel(&u.unit_type)),
                    format!("{contrib:.2}{detail}"),
                )
            })
            .collect(),
        def_sum,
        def_bonus,
        battle.defender_initial_fp,
    );

    // Range first-strike volley.
    let atk_max = battle
        .attacker_unit_logs
        .iter()
        .map(|u| unit_range(&u.unit_type))
        .max()
        .unwrap_or(0);
    let def_max = battle
        .defender_unit_logs
        .iter()
        .map(|u| unit_range(&u.unit_type))
        .max()
        .unwrap_or(0);
    parent.spawn((
        Text::new(format!(
            "Range first-strike: attacker max range {atk_max} vs defender max range {def_max}."
        )),
        theme.font_bold(12.5),
        TextColor(NUM_WHITE),
    ));
    let volley = if atk_max == def_max {
        "No first-strike volley fired (ranges are equal).".to_string()
    } else {
        let (side, logs, opp) = if atk_max > def_max {
            ("Attacker", &battle.attacker_unit_logs, def_max)
        } else {
            ("Defender", &battle.defender_unit_logs, atk_max)
        };
        let qualified: Vec<&BattleUnitLogVm> = logs
            .iter()
            .filter(|u| unit_range(&u.unit_type) > opp)
            .collect();
        let volley_fp: f64 = qualified.iter().map(|u| u.initial_firepower).sum();
        format!(
            "{side} fires one free volley before round 1 with {} over-range unit{} \
             (range > {opp}), volley FP {volley_fp:.2}.",
            qualified.len(),
            if qualified.len() == 1 { "" } else { "s" }
        )
    };
    parent.spawn((
        Text::new(volley),
        theme.font(11.0),
        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
    ));

    parent.spawn((
        Text::new(
            "Damage exchange (each round): each unit picks one enemy target and concentrates \
             its firepower on it. Front-line units (infantry / cavalry / garrison) target the \
             enemy front-line first, falling through to artillery only if the front-line is \
             wiped. Artillery targets enemy artillery first, falling through to front-line. \
             Damage spills to the next priority target on overkill, so a stack always finishes \
             off wounded units before the next one. Up to 10 rounds; ends early on wipeout or \
             FP-loss retreat.",
        ),
        theme.font_italic(11.0),
        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
    ));
}

// ── Round-by-round playout (debug) ───────────────────────────────────────

fn round_playout(parent: &mut ChildSpawnerCommands, theme: &Theme, battle: &LandBattleVm) {
    parent.spawn((
        Text::new(
            "Per-round trace from the resolver. Each shot picks one priority target \
             (front-line shooters target enemy front-line; artillery targets enemy artillery) \
             and damage spills to the next on overkill.",
        ),
        theme.font_italic(11.0),
        TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
    ));
    let casualties = |list: &[String]| -> String {
        if list.is_empty() {
            "—".to_string()
        } else {
            list.iter()
                .map(|c| split_camel(c))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    for round in &battle.round_logs {
        let volley = round.round == 0;
        let title = if volley {
            format!(
                "First-strike volley — {} fires",
                round.first_strike_side.as_deref().unwrap_or("?")
            )
        } else {
            format!("Round {}", round.round)
        };
        parent
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    border: UiRect::left(Val::Px(3.0)),
                    padding: UiRect::left(Val::Px(8.0)),
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                },
                BorderColor::all(if volley {
                    Color::srgb_u8(0xd4, 0xa5, 0x2a)
                } else {
                    Color::srgb_u8(0x55, 0x55, 0x55)
                }),
            ))
            .with_children(|block| {
                block
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    })
                    .with_children(|line| {
                        line.spawn((
                            Text::new(title),
                            theme.font_bold(12.0),
                            TextColor(theme::TEXT),
                        ));
                        if let Some(retreater) = round.retreat_triggered.as_deref() {
                            line.spawn((
                                Text::new(format!(
                                    "→ {retreater} retreats (FP loss past threshold; +10% damage \
                                     on the way out)"
                                )),
                                theme.font(11.0),
                                TextColor(Color::srgb_u8(0xff, 0xb3, 0x8a)),
                            ));
                        }
                    });
                if volley {
                    let attacker_fires = round.first_strike_side.as_deref() == Some("attacker");
                    let (fp, shots) = if attacker_fires {
                        (round.atk_fp, round.atk_shots)
                    } else {
                        (round.def_fp, round.def_shots)
                    };
                    block.spawn((
                        Text::new(format!(
                            "Volley FP: {fp:.2} from {shots} over-range shooter(s)"
                        )),
                        theme.font(11.0),
                        TextColor(NUM_WHITE),
                    ));
                    let hit = if attacker_fires {
                        &round.def_casualties
                    } else {
                        &round.atk_casualties
                    };
                    block.spawn((
                        Text::new(format!("Casualties: {}", casualties(hit))),
                        theme.font(11.0),
                        TextColor(LABEL_BLUE),
                    ));
                } else {
                    block.spawn((
                        Text::new(format!(
                            "Attacker fire: {:.2} from {} shooter{}",
                            round.atk_fp,
                            round.atk_shots,
                            if round.atk_shots == 1 { "" } else { "s" }
                        )),
                        theme.font(11.0),
                        TextColor(Color::srgb_u8(0x9e, 0xcb, 0xff)),
                    ));
                    block.spawn((
                        Text::new(format!(
                            "Defender fire: {:.2} from {} shooter{}",
                            round.def_fp,
                            round.def_shots,
                            if round.def_shots == 1 { "" } else { "s" }
                        )),
                        theme.font(11.0),
                        TextColor(Color::srgb_u8(0xff, 0xb3, 0x8a)),
                    ));
                    block.spawn((
                        Text::new(format!(
                            "Atk casualties: {}",
                            casualties(&round.atk_casualties)
                        )),
                        theme.font(11.0),
                        TextColor(LABEL_BLUE),
                    ));
                    block.spawn((
                        Text::new(format!(
                            "Def casualties: {}",
                            casualties(&round.def_casualties)
                        )),
                        theme.font(11.0),
                        TextColor(LABEL_BLUE),
                    ));
                }
            });
    }
}

// ── Naval battle details ─────────────────────────────────────────────────

/// `["Frigate", "Frigate", "Raider"]` → `"2x Frigate, Raider"`.
fn dedup_ships(types: &[String]) -> String {
    let mut order: Vec<&str> = Vec::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for t in types {
        let entry = counts.entry(t.as_str()).or_insert(0);
        if *entry == 0 {
            order.push(t);
        }
        *entry += 1;
    }
    order
        .iter()
        .map(|t| {
            let count = counts[*t];
            if count > 1 {
                format!("{count}x {}", split_camel(t))
            } else {
                split_camel(t)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn naval_battle_details(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    flags: &FlagCache,
    battle: &NavalBattleVm,
) {
    let (winner_name, winner_id) = if battle.attacker_won {
        (&battle.attacker, battle.attacker_id)
    } else {
        (&battle.defender, battle.defender_id)
    };
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|details| {
            details
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::axes(Val::Px(20.0), Val::Px(12.0)),
                        border: UiRect::left(Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(SECTION_BG),
                    BorderColor::all(if battle.attacker_won {
                        WIN_GREEN
                    } else {
                        LOSS_RED
                    }),
                ))
                .with_children(|banner| {
                    banner
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|title| {
                            if let Some(flag) = flags.get(winner_id) {
                                title.spawn((
                                    Node {
                                        width: Val::Px(24.0),
                                        height: Val::Px(16.0),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                    ImageNode::new(flag),
                                ));
                            }
                            title.spawn((
                                Text::new(format!("{winner_name} Naval Victory")),
                                theme.font_bold(19.0),
                                TextColor(theme::GOLD),
                            ));
                        });
                    banner.spawn((
                        Text::new(format!("{} vs {}", battle.attacker, battle.defender)),
                        theme.font(13.0),
                        TextColor(SUB_GRAY),
                    ));
                });

            section(details, theme, "FLEETS", |body, theme| {
                body.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(24.0),
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|grid| {
                    for (name, id, survivors, lost) in [
                        (
                            &battle.attacker,
                            battle.attacker_id,
                            battle.attacker_survivors_count,
                            &battle.attacker_ships_lost,
                        ),
                        (
                            &battle.defender,
                            battle.defender_id,
                            battle.defender_survivors_count,
                            &battle.defender_ships_lost,
                        ),
                    ] {
                        grid.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            flex_basis: Val::Percent(50.0),
                            flex_grow: 1.0,
                            row_gap: Val::Px(3.0),
                            ..default()
                        })
                        .with_children(|column| {
                            column
                                .spawn(Node {
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(6.0),
                                    ..default()
                                })
                                .with_children(|header| {
                                    if let Some(flag) = flags.get(id) {
                                        header.spawn((
                                            Node {
                                                width: Val::Px(20.0),
                                                height: Val::Px(13.0),
                                                flex_shrink: 0.0,
                                                ..default()
                                            },
                                            ImageNode::new(flag),
                                        ));
                                    }
                                    header.spawn((
                                        Text::new(name.clone()),
                                        theme.font_bold(13.0),
                                        TextColor(theme::TEXT),
                                    ));
                                });
                            column.spawn((
                                Text::new(format!("{survivors} ships survived")),
                                theme.font(12.0),
                                TextColor(Color::srgb_u8(0xbb, 0xbb, 0xbb)),
                            ));
                            if !lost.is_empty() {
                                column.spawn((
                                    Text::new(format!("Lost: {}", dedup_ships(lost))),
                                    theme.font(12.0),
                                    TextColor(LOSS_RED),
                                ));
                            }
                        });
                    }
                });
            });
        });
}
