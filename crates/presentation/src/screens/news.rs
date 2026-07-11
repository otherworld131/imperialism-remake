//! Newspaper screen (F8, web `NewspaperScreen`): "The Imperial Times"
//! interstitial that auto-opens after every end turn with the resolved
//! turn's headlines, split "Your Empire" vs "World News" with category color
//! bars and nation tags. Filters (category / country / text search), debug
//! AI-reasoning and non-action reveals, and a lazily loaded Archive tab with
//! a per-turn political-map modal. Dismissing the paper returns to the map
//! and opens the proposal modal when proposals arrived with the turn.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::game::resources::{
    CurrentTurnNews, DeferredProposals, GameMeta, NewsArchive, NewsDebugSettings, ProposalPrompt,
    SessionRes, TurnInfo, ViewModels,
};
use crate::game::vm::{self, HeadlineVm, PoliticalSnapshotVm};
use crate::map::picking::PickingBlocker;
use crate::screens::minimap::{self, HexCell, Raster, RasterParams};
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, DropdownChanged, DropdownProps, ModalProps, ModalStack,
    ScrollProps, TextInputChanged, TextInputProps, Toast,
};

// ── Newspaper palette (web `NewspaperScreen` styles) ─────────────────────

const PAPER_BG: Color = Color::srgb_u8(0xfa, 0xf5, 0xe8);
const PAPER_TOOLBAR_BG: Color = Color::srgb_u8(0xf0, 0xea, 0xd6);
const PAPER_SIDEBAR_BG: Color = Color::srgb_u8(0xf5, 0xef, 0xe0);
const PAPER_INK: Color = Color::srgb_u8(0x1a, 0x1a, 0x1a);
const PAPER_DIM: Color = Color::srgb_u8(0x66, 0x66, 0x66);
const PAPER_FAINT: Color = Color::srgb_u8(0x99, 0x99, 0x99);
const PAPER_RULE: Color = Color::srgb_u8(0x33, 0x33, 0x33);
const PAPER_BROWN: Color = Color::srgb_u8(0x8b, 0x45, 0x13);

/// Web `CATEGORY_COLORS` (keys are lowercase category names).
fn category_color(category: &str) -> Color {
    match category.to_ascii_lowercase().as_str() {
        "war" => Color::srgb_u8(0xe6, 0x39, 0x46),
        "battle" => Color::srgb_u8(0xe7, 0x6f, 0x51),
        "diplomacy" => Color::srgb_u8(0x45, 0x7b, 0x9d),
        "growth" => Color::srgb_u8(0x2a, 0x9d, 0x8f),
        "trade" => Color::srgb_u8(0xda, 0xa5, 0x20),
        "crisis" => Color::srgb_u8(0x9d, 0x02, 0x08),
        "politics" => Color::srgb_u8(0xb3, 0x80, 0xe6),
        "military" => Color::srgb_u8(0x8a, 0x9a, 0xaf),
        _ => Color::srgb_u8(0x33, 0x33, 0x33),
    }
}

/// `(filter value, dropdown label)` — web `NEWS_CATEGORY_OPTIONS`.
const CATEGORY_OPTIONS: [(&str, &str); 10] = [
    ("all", "All topics"),
    ("war", "War"),
    ("battle", "Battle"),
    ("diplomacy", "Diplomacy"),
    ("growth", "Growth"),
    ("trade", "Trade"),
    ("crisis", "Crisis"),
    ("politics", "Politics"),
    ("military", "Military"),
    ("default", "Other"),
];

// ── Screen state ─────────────────────────────────────────────────────────

/// Newspaper UI state. Mode and archive selection reset on every open (the
/// web component remounts); the filters persist like the web App state, the
/// text search clearing when a new turn report arrives.
#[derive(Resource, Default)]
pub struct NewsUi {
    pub archive_mode: bool,
    pub selected_turn: Option<u32>,
    /// Index into [`CATEGORY_OPTIONS`].
    pub category: usize,
    /// Country filter by nation id (`None` = all countries).
    pub country: Option<i64>,
    pub search: String,
    /// Turn number whose report the search box was last cleared for.
    seen_report: u32,
    /// Country-dropdown option ids, parallel to the spawned options
    /// (index 0 = All countries).
    country_ids: Vec<i64>,
}

#[derive(Component)]
pub struct NewsRoot;

#[derive(Component)]
pub struct NewsDateText;

/// Mode tab: `false` = Current, `true` = Archive.
#[derive(Component)]
pub struct NewsModeTab(pub bool);

#[derive(Component)]
pub struct NewsCategoryDropdown;

#[derive(Component)]
pub struct NewsCountryDropdown;

#[derive(Component)]
pub struct NewsSearchInput;

#[derive(Component)]
pub struct NewsShowMapButton;

#[derive(Component)]
pub struct NewsSidebar;

#[derive(Component)]
pub struct NewsArchiveTurnButton(pub u32);

#[derive(Component)]
pub struct NewsHeadlines;

#[derive(Component)]
pub struct NewsBackButton;

#[derive(Component)]
pub struct NewsContinueButton;

// ── Enter / exit ─────────────────────────────────────────────────────────

pub fn enter_news(
    mut commands: Commands,
    theme: Res<Theme>,
    vms: Res<ViewModels>,
    news: Res<CurrentTurnNews>,
    mut ui: ResMut<NewsUi>,
) {
    ui.archive_mode = false;
    ui.selected_turn = None;
    if news.turn_number != ui.seen_report {
        ui.search.clear();
        ui.seen_report = news.turn_number;
    }

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(44.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(PAPER_BG),
            GlobalZIndex(50),
            Interaction::default(),
            PickingBlocker,
            NewsRoot,
        ))
        .id();

    commands.entity(root).with_children(|panel| {
        // Masthead, with the uniform "Close (Esc)" top-right (CC-5).
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(32.0), Val::Px(10.0)),
                    border: UiRect::bottom(Val::Px(3.0)),
                    ..default()
                },
                BorderColor::all(PAPER_RULE),
            ))
            .with_children(|masthead| {
                masthead.spawn((
                    Text::new("The Imperial Times"),
                    theme.font_blackletter(40.0),
                    TextColor(PAPER_INK),
                ));
                masthead.spawn((
                    Text::new(""),
                    theme.font(13.0),
                    TextColor(PAPER_DIM),
                    NewsDateText,
                ));
                masthead
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(10.0),
                        right: Val::Px(16.0),
                        ..default()
                    })
                    .with_children(|cell| {
                        let button = widgets::spawn_button(
                            cell,
                            &theme,
                            ButtonProps {
                                label: "Close (Esc)".into(),
                                font_size: 13.0,
                                ..default()
                            },
                        );
                        cell.commands().entity(button).insert(NewsBackButton);
                    });
            });

        // Toolbar: mode tabs + filters.
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(24.0), Val::Px(6.0)),
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(PAPER_TOOLBAR_BG),
                BorderColor::all(Color::srgb_u8(0xcc, 0xcc, 0xcc)),
            ))
            .with_children(|toolbar| {
                toolbar
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|tabs| {
                        for (archive, label) in [(false, "Current"), (true, "Archive")] {
                            let button = widgets::spawn_button(
                                tabs,
                                &theme,
                                ButtonProps {
                                    label: label.into(),
                                    font_size: 13.0,
                                    auto_label_tint: false,
                                    ..default()
                                },
                            );
                            tabs.commands().entity(button).insert(NewsModeTab(archive));
                        }
                    });
                toolbar
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0),
                        ..default()
                    })
                    .with_children(|filters| {
                        let category = widgets::spawn_dropdown(
                            filters,
                            &theme,
                            DropdownProps {
                                options: CATEGORY_OPTIONS
                                    .iter()
                                    .map(|(_, label)| (*label).to_string())
                                    .collect(),
                                selected: ui.category,
                                width: Val::Px(130.0),
                            },
                        );
                        filters
                            .commands()
                            .entity(category)
                            .insert(NewsCategoryDropdown);

                        // Country options, captured for the change handler.
                        let mut country_options = vec!["All countries".to_string()];
                        ui.country_ids.clear();
                        ui.country_ids.push(-1);
                        for nation in &vms.nations {
                            country_options.push(nation.name.clone());
                            ui.country_ids.push(i64::from(nation.nation_id));
                        }
                        let selected_country = ui
                            .country
                            .and_then(|id| ui.country_ids.iter().position(|&c| c == id))
                            .unwrap_or(0);
                        let country = widgets::spawn_dropdown(
                            filters,
                            &theme,
                            DropdownProps {
                                options: country_options,
                                selected: selected_country,
                                width: Val::Px(150.0),
                            },
                        );
                        filters
                            .commands()
                            .entity(country)
                            .insert(NewsCountryDropdown);

                        let search = widgets::spawn_text_input(
                            filters,
                            &theme,
                            TextInputProps {
                                width: Val::Px(160.0),
                                max_len: 48,
                                value: ui.search.clone(),
                                placeholder: "Filter…".into(),
                            },
                        );
                        filters.commands().entity(search).insert(NewsSearchInput);

                        let show_map = widgets::spawn_button(
                            filters,
                            &theme,
                            ButtonProps {
                                label: "Show Map".into(),
                                font_size: 13.0,
                                ..default()
                            },
                        );
                        filters.commands().entity(show_map).insert((
                            NewsShowMapButton,
                            widgets::TooltipText("Political map at this turn".into()),
                        ));
                    });
            });

        // Content: archive sidebar + headlines.
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                ..default()
            })
            .with_children(|content| {
                content.spawn((
                    Node {
                        display: Display::None,
                        width: Val::Px(190.0),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        border: UiRect::right(Val::Px(1.0)),
                        padding: UiRect::vertical(Val::Px(8.0)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(PAPER_SIDEBAR_BG),
                    BorderColor::all(Color::srgb_u8(0xcc, 0xcc, 0xcc)),
                    NewsSidebar,
                ));
                let scroll = widgets::spawn_scroll_area(
                    content,
                    &theme,
                    ScrollProps {
                        flex_grow: 1.0,
                        ..default()
                    },
                );
                content
                    .commands()
                    .entity(scroll.content)
                    .insert(NewsHeadlines);
            });

        // Footer: primary action bottom-right (CC-5).
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(24.0), Val::Px(8.0)),
                    border: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(PAPER_TOOLBAR_BG),
                BorderColor::all(PAPER_RULE),
            ))
            .with_children(|footer| {
                let cont = widgets::spawn_button(
                    footer,
                    &theme,
                    ButtonProps {
                        label: "Continue".into(),
                        font_size: 14.0,
                        width: Some(Val::Px(130.0)),
                        ..default()
                    },
                );
                footer.commands().entity(cont).insert(NewsContinueButton);
            });
    });
}

/// Close the paper; if proposals arrived with the last turn, open the
/// proposal modal now (web `dismissNewspaper` order).
pub fn exit_news(
    mut commands: Commands,
    roots: Query<Entity, With<NewsRoot>>,
    mut deferred: ResMut<DeferredProposals>,
    mut prompt: ResMut<ProposalPrompt>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Some(proposals) = deferred.0.take() {
        prompt.0 = Some(proposals);
    }
}

// ── Filtering (web `applyNewsFilters`) ───────────────────────────────────

fn headline_visible(h: &HeadlineVm, ui: &NewsUi, debug: &NewsDebugSettings) -> bool {
    if !debug.show_ai_non_actions && h.is_non_action {
        return false;
    }
    let (category_value, _) = CATEGORY_OPTIONS[ui.category.min(CATEGORY_OPTIONS.len() - 1)];
    if category_value != "all" {
        let category = if h.category.is_empty() {
            "default".to_string()
        } else {
            h.category.to_ascii_lowercase()
        };
        if category != category_value {
            return false;
        }
    }
    if let Some(country) = ui.country
        && !h.nation_ids.contains(&country)
    {
        return false;
    }
    let query = ui.search.trim().to_lowercase();
    if !query.is_empty()
        && !h.text.to_lowercase().contains(&query)
        && !h
            .reason
            .as_deref()
            .is_some_and(|r| r.to_lowercase().contains(&query))
    {
        return false;
    }
    true
}

// ── Rebuild ──────────────────────────────────────────────────────────────

/// Light-touch chrome updates: masthead date, mode-tab tint + archive
/// count, and the Show Map / Continue button visibility.
pub fn update_news_chrome(
    ui: Res<NewsUi>,
    news: Res<CurrentTurnNews>,
    archive: Res<NewsArchive>,
    turn_info: Res<TurnInfo>,
    mut dates: Query<&mut Text, With<NewsDateText>>,
    mode_tabs: Query<(&NewsModeTab, &Children)>,
    show_map_buttons: Query<Entity, With<NewsShowMapButton>>,
    continue_buttons: Query<Entity, With<NewsContinueButton>>,
    mut texts: Query<&mut Text, Without<NewsDateText>>,
    mut label_colors: Query<&mut TextColor>,
    mut nodes: Query<&mut Node>,
) {
    if !ui.is_changed() && !news.is_changed() && !archive.is_changed() {
        return;
    }
    // Selected archive entry (archive mode only).
    let selected_entry = ui
        .selected_turn
        .and_then(|turn| archive.entries.iter().find(|e| e.turn == turn));

    // Masthead date: the archived turn, the freshest report's turn, or the
    // live turn label before any turn has resolved.
    let date_label = if ui.archive_mode && selected_entry.is_some() {
        selected_entry
            .map(|e| format!("{} Q{} — Turn {}", e.year, e.quarter, e.turn))
            .unwrap_or_default()
    } else if news.has_report {
        format!(
            "{} Q{} — Turn {}",
            news.year, news.quarter, news.turn_number
        )
    } else {
        turn_info.label.clone()
    };
    for mut date in &mut dates {
        **date = date_label.clone();
    }

    // Mode tab tint + archive count label.
    for (tab, children) in &mode_tabs {
        let active = tab.0 == ui.archive_mode;
        for child in children {
            if tab.0
                && let Ok(mut text) = texts.get_mut(*child)
            {
                **text = format!("Archive ({})", archive.entries.len());
            }
            if let Ok(mut color) = label_colors.get_mut(*child) {
                color.0 = if active { theme::GOLD } else { theme::TEXT_DIM };
            }
        }
    }

    // Show Map button only with an archive turn selected.
    for entity in &show_map_buttons {
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.display = if ui.archive_mode && ui.selected_turn.is_some() {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    // Continue button only in current mode (web parity).
    for entity in &continue_buttons {
        if let Ok(mut node) = nodes.get_mut(entity) {
            node.display = if ui.archive_mode {
                Display::None
            } else {
                Display::Flex
            };
        }
    }
}

/// Rebuild the archive sidebar and the headline columns.
pub fn update_news_content(
    mut commands: Commands,
    theme: Res<Theme>,
    vms: Res<ViewModels>,
    meta: Res<GameMeta>,
    news: Res<CurrentTurnNews>,
    archive: Res<NewsArchive>,
    debug: Res<NewsDebugSettings>,
    ui: Res<NewsUi>,
    mut sidebars: Query<(Entity, &mut Node), With<NewsSidebar>>,
    headlines_areas: Query<Entity, With<NewsHeadlines>>,
    added: Query<(), Added<NewsHeadlines>>,
) {
    if !ui.is_changed()
        && !news.is_changed()
        && !archive.is_changed()
        && !debug.is_changed()
        && added.is_empty()
    {
        return;
    }
    let Ok(headlines_area) = headlines_areas.single() else {
        return;
    };
    let selected_entry = ui
        .selected_turn
        .and_then(|turn| archive.entries.iter().find(|e| e.turn == turn));

    // Archive sidebar.
    if let Ok((sidebar, mut node)) = sidebars.single_mut() {
        node.display = if ui.archive_mode {
            Display::Flex
        } else {
            Display::None
        };
        commands.entity(sidebar).despawn_children();
        if ui.archive_mode {
            commands.entity(sidebar).with_children(|list| {
                if !archive.loaded {
                    list.spawn((
                        Text::new("Loading reports…"),
                        theme.font(12.0),
                        TextColor(PAPER_DIM),
                        Node {
                            margin: UiRect::all(Val::Px(12.0)),
                            ..default()
                        },
                    ));
                } else if archive.entries.is_empty() {
                    list.spawn((
                        Text::new("No reports yet"),
                        theme.font(12.0),
                        TextColor(PAPER_DIM),
                        Node {
                            margin: UiRect::all(Val::Px(12.0)),
                            ..default()
                        },
                    ));
                }
                let mut entries: Vec<_> = archive.entries.iter().collect();
                entries.sort_by(|a, b| b.turn.cmp(&a.turn));
                for entry in entries {
                    let active = ui.selected_turn == Some(entry.turn);
                    let prefix = if active { "▶ " } else { "" };
                    let button = widgets::spawn_button(
                        list,
                        &theme,
                        ButtonProps {
                            label: format!(
                                "{prefix}Turn {} ({} Q{})",
                                entry.turn, entry.year, entry.quarter
                            ),
                            font_size: 12.5,
                            ..default()
                        },
                    );
                    list.commands()
                        .entity(button)
                        .insert(NewsArchiveTurnButton(entry.turn));
                }
            });
        }
    }

    // Headlines.
    commands.entity(headlines_area).despawn_children();
    let shown: Option<&[HeadlineVm]> = if ui.archive_mode {
        selected_entry.map(|e| e.headlines.as_slice())
    } else {
        news.has_report.then_some(news.headlines.as_slice())
    };
    let player_name = vms
        .nations
        .iter()
        .find(|n| n.nation_id == meta.player_nation)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let gp_names: Vec<&str> = vms
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.name.as_str())
        .collect();
    let all_nation_names: Vec<&str> = vms.nations.iter().map(|n| n.name.as_str()).collect();

    commands.entity(headlines_area).with_children(|area| {
        let pad = Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(Val::Px(32.0), Val::Px(16.0)),
            row_gap: Val::Px(18.0),
            width: Val::Percent(100.0),
            ..default()
        };
        let mut body = area.spawn(pad);
        body.with_children(|body| {
            let Some(shown) = shown else {
                let message = if ui.archive_mode {
                    "Select a turn from the sidebar to view its headlines."
                } else {
                    "No turn has resolved yet — end a turn to print the first edition."
                };
                body.spawn((
                    Text::new(message),
                    theme.font_italic(13.0),
                    TextColor(PAPER_FAINT),
                ));
                return;
            };
            let visible: Vec<&HeadlineVm> = shown
                .iter()
                .filter(|h| headline_visible(h, &ui, &debug))
                .collect();
            let (player_news, world_news): (Vec<&HeadlineVm>, Vec<&HeadlineVm>) = visible
                .iter()
                .partition(|h| !player_name.is_empty() && h.text.contains(&player_name));
            if player_news.is_empty() && world_news.is_empty() {
                body.spawn((
                    Text::new("No headlines match the current filters."),
                    theme.font_italic(13.0),
                    TextColor(PAPER_FAINT),
                ));
                return;
            }
            if !player_news.is_empty() {
                headline_section(
                    body,
                    &theme,
                    &format!("YOUR EMPIRE — {}", player_name.to_uppercase()),
                    PAPER_RULE,
                    &player_news,
                    &[],
                    &all_nation_names,
                    debug.show_ai_reasoning,
                );
            }
            if !world_news.is_empty() {
                headline_section(
                    body,
                    &theme,
                    "WORLD NEWS",
                    PAPER_DIM,
                    &world_news,
                    &gp_names,
                    &all_nation_names,
                    debug.show_ai_reasoning,
                );
            }
        });
    });
}

/// Section label + two-column headline flow (the web uses CSS multi-columns;
/// here each section's items split half-and-half across two columns).
#[allow(clippy::too_many_arguments)]
fn headline_section(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    label: &str,
    label_color: Color,
    headlines: &[&HeadlineVm],
    nation_tags: &[&str],
    all_nation_names: &[&str],
    show_reasoning: bool,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|section| {
            section
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        border: UiRect::bottom(Val::Px(2.0)),
                        padding: UiRect::bottom(Val::Px(3.0)),
                        margin: UiRect::bottom(Val::Px(10.0)),
                        ..default()
                    },
                    BorderColor::all(label_color),
                ))
                .with_children(|header| {
                    header.spawn((
                        Text::new(label),
                        theme.font_bold(12.0),
                        TextColor(label_color),
                    ));
                });
            // Coalesce near-identical headlines (same category + same text
            // once nation names are masked) into one line with the detail
            // list behind a tooltip.
            let grouped = coalesce_headlines(headlines, all_nation_names);
            section
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(32.0),
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|columns| {
                    let split = grouped.len().div_ceil(2);
                    for chunk in [&grouped[..split], &grouped[split..]] {
                        columns
                            .spawn(Node {
                                flex_direction: FlexDirection::Column,
                                flex_basis: Val::Percent(50.0),
                                flex_grow: 1.0,
                                min_width: Val::Px(0.0),
                                row_gap: Val::Px(4.0),
                                ..default()
                            })
                            .with_children(|column| {
                                for group in chunk {
                                    headline_row(column, theme, group, nation_tags, show_reasoning);
                                }
                            });
                    }
                });
        });
}

/// One rendered headline: the lead item plus any coalesced repeats.
struct HeadlineGroup<'a> {
    lead: &'a HeadlineVm,
    /// Texts of the coalesced repeats (empty = a normal single headline).
    others: Vec<&'a str>,
}

/// Group headlines whose text differs only by nation names, preserving
/// first-seen order (view-model-level grouping; no more than one line per
/// event type per turn).
fn coalesce_headlines<'a>(
    headlines: &[&'a HeadlineVm],
    nation_names: &[&str],
) -> Vec<HeadlineGroup<'a>> {
    // Ordered placeholders: the first distinct nation in the text becomes
    // `#1`, the second `#2`, … so templates keep their argument structure
    // while still merging the "same event, different nations" repeats the
    // plan calls for.
    let template = |text: &str| -> String {
        let mut found: Vec<(usize, &str)> = nation_names
            .iter()
            .filter(|name| !name.is_empty())
            .filter_map(|name| text.find(name).map(|pos| (pos, *name)))
            .collect();
        found.sort();
        let mut masked = text.to_string();
        for (index, (_, name)) in found.iter().enumerate() {
            masked = masked.replace(name, &format!("#{}", index + 1));
        }
        masked
    };
    let mut order: Vec<(String, HeadlineGroup<'a>)> = Vec::new();
    for headline in headlines {
        let key = format!("{}|{}", headline.category, template(&headline.text));
        match order.iter_mut().find(|(k, _)| *k == key) {
            Some((_, group)) => group.others.push(headline.text.as_str()),
            None => order.push((
                key,
                HeadlineGroup {
                    lead: headline,
                    others: Vec::new(),
                },
            )),
        }
    }
    order.into_iter().map(|(_, group)| group).collect()
}

fn headline_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    group: &HeadlineGroup,
    nation_tags: &[&str],
    show_reasoning: bool,
) {
    let headline = group.lead;
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                border: UiRect::left(Val::Px(3.0)),
                padding: UiRect::new(Val::Px(12.0), Val::Px(0.0), Val::Px(4.0), Val::Px(4.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BorderColor::all(category_color(&headline.category)),
            // CC-4: the colored edge bar gets a label.
            widgets::TooltipText(format!("Category: {}", capitalize(&headline.category))),
        ))
        .with_children(|row| {
            // Nation tag (web `extractNationTag`: first Great Power named in
            // the text) — world-news rows only.
            let tag = nation_tags
                .iter()
                .find(|name| headline.text.contains(**name));
            if let Some(tag) = tag {
                row.spawn((
                    Text::new(tag.to_uppercase()),
                    theme.font_bold(10.0),
                    TextColor(PAPER_BROWN),
                ));
            }
            row.spawn((
                Text::new(headline.text.clone()),
                theme.font(13.5),
                TextColor(PAPER_INK),
            ));
            if !group.others.is_empty() {
                row.spawn((
                    Text::new(format!(
                        "+{} similar report{}",
                        group.others.len(),
                        if group.others.len() == 1 { "" } else { "s" }
                    )),
                    theme.font_italic(11.5),
                    TextColor(PAPER_DIM),
                    widgets::TooltipText(group.others.join("\n")),
                ));
            }
            if show_reasoning && let Some(reason) = headline.reason.as_deref() {
                row.spawn((
                    Text::new(reason.to_string()),
                    theme.font_italic(12.0),
                    TextColor(Color::srgb_u8(0x88, 0x88, 0x88)),
                ));
            }
        });
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ── Interactions ─────────────────────────────────────────────────────────

pub fn handle_news_buttons(
    mut activations: MessageReader<ButtonActivated>,
    mode_tabs: Query<&NewsModeTab>,
    archive_turns: Query<&NewsArchiveTurnButton>,
    show_map: Query<(), With<NewsShowMapButton>>,
    back: Query<(), With<NewsBackButton>>,
    cont: Query<(), With<NewsContinueButton>>,
    mut ui: ResMut<NewsUi>,
    mut archive: ResMut<NewsArchive>,
    session: Res<SessionRes>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut commands: Commands,
    theme: Res<Theme>,
    mut modal_stack: ResMut<ModalStack>,
    mut images: ResMut<Assets<Image>>,
    mut toasts: MessageWriter<Toast>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(tab) = mode_tabs.get(*entity) {
            ui.archive_mode = tab.0;
            if tab.0 {
                load_archive_increment(&session, &mut archive);
            }
        } else if let Ok(turn) = archive_turns.get(*entity) {
            ui.selected_turn = Some(turn.0);
        } else if show_map.contains(*entity) {
            let Some(turn) = ui.selected_turn else {
                continue;
            };
            let Some(session) = session.0.as_ref() else {
                continue;
            };
            match frontend_api::map::get_political_snapshot(session.game(), turn)
                .map(vm::parse_political_snapshot)
            {
                Ok(Ok(snapshot)) => open_political_map_modal(
                    &mut commands,
                    &theme,
                    &mut modal_stack,
                    &mut images,
                    &snapshot,
                ),
                Ok(Err(err)) => {
                    toasts.write(Toast::error(format!("Snapshot decode failed: {err}")));
                }
                Err(err) => {
                    toasts.write(Toast::error(format!(
                        "No political snapshot available for turn {turn} ({}).",
                        err.message()
                    )));
                }
            }
        } else if back.contains(*entity) || cont.contains(*entity) {
            next_screen.set(Screen::Map);
        }
    }
}

pub fn handle_news_filters(
    mut dropdowns: MessageReader<DropdownChanged>,
    mut text_inputs: MessageReader<TextInputChanged>,
    categories: Query<(), With<NewsCategoryDropdown>>,
    countries: Query<(), With<NewsCountryDropdown>>,
    searches: Query<(), With<NewsSearchInput>>,
    mut ui: ResMut<NewsUi>,
) {
    for change in dropdowns.read() {
        if categories.contains(change.entity) {
            ui.category = change.index.min(CATEGORY_OPTIONS.len() - 1);
        } else if countries.contains(change.entity) {
            let id = ui.country_ids.get(change.index).copied().unwrap_or(-1);
            ui.country = (id >= 0).then_some(id);
        }
    }
    for change in text_inputs.read() {
        if searches.contains(change.entity) && ui.search != change.value {
            ui.search = change.value.clone();
        }
    }
}

/// Incremental archive load: fetch only the turns after `loaded_through`
/// (ports the web's per-turn cache; re-fetching on every Archive open picks
/// up turns resolved since).
fn load_archive_increment(session: &SessionRes, archive: &mut NewsArchive) {
    let Some(session) = session.0.as_ref() else {
        return;
    };
    match frontend_api::newspaper::get_newspaper_archive_since(
        session.game(),
        archive.loaded_through,
    )
    .map(vm::parse_newspaper_archive)
    {
        Ok(Ok(entries)) => {
            for entry in entries {
                archive.loaded_through = archive.loaded_through.max(entry.turn);
                archive.entries.push(entry);
            }
            archive.loaded = true;
        }
        Ok(Err(err)) => warn!("newspaper-archive decode failed: {err}"),
        Err(err) => warn!("get_newspaper_archive_since failed: {}", err.message()),
    }
}

// ── Political-map modal (web `PoliticalMapModal`) ────────────────────────

const POLITICAL_MAP_W: f32 = 720.0;
const POLITICAL_MAP_H: f32 = 480.0;

fn open_political_map_modal(
    commands: &mut Commands,
    theme: &Theme,
    modal_stack: &mut ModalStack,
    images: &mut Assets<Image>,
    snapshot: &PoliticalSnapshotVm,
) {
    let handles = widgets::open_modal(
        commands,
        modal_stack,
        theme,
        ModalProps {
            title: format!(
                "Political Map — {} Q{} (Turn {})",
                snapshot.year, snapshot.quarter, snapshot.turn
            ),
            width: Val::Px(POLITICAL_MAP_W + 240.0),
        },
    );

    // Fit the whole map into the panel: bounds at unit hex size pick the
    // scale, the offset centers the content.
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for tile in &snapshot.tiles {
        let p = minimap::hex_to_px(tile.q, tile.r, 1.0);
        min = min.min(p);
        max = max.max(p);
    }
    if snapshot.tiles.is_empty() {
        min = Vec2::ZERO;
        max = Vec2::ONE;
    }
    let span = (max - min) + Vec2::splat(4.0);
    let hex_size = (POLITICAL_MAP_W / span.x)
        .min(POLITICAL_MAP_H / span.y)
        .max(1.0);
    let center = (min + max) / 2.0 * hex_size;
    let offset = Vec2::new(POLITICAL_MAP_W, POLITICAL_MAP_H) / 2.0 - center;

    // Border groups: country = visual group (or owner), province = name.
    let mut country_ids: HashMap<&str, u32> = HashMap::new();
    let mut province_ids: HashMap<&str, u32> = HashMap::new();
    let mut cells: HashMap<(i32, i32), HexCell> = HashMap::new();
    for tile in &snapshot.tiles {
        let fill = if tile.terrain == "Sea" {
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
        let owned_land = tile.terrain != "Sea" && !tile.owner.is_empty();
        let country = if owned_land {
            let next = country_ids.len() as u32;
            *country_ids
                .entry(tile.visual_group_or_owner())
                .or_insert(next)
        } else {
            u32::MAX
        };
        let province = if owned_land {
            let next = province_ids.len() as u32;
            *province_ids.entry(tile.province.as_str()).or_insert(next)
        } else {
            0
        };
        cells.insert(
            (tile.q, tile.r),
            HexCell {
                fill,
                country,
                province,
            },
        );
    }

    let mut raster = Raster::new(
        RasterParams {
            width: POLITICAL_MAP_W as u32,
            height: POLITICAL_MAP_H as u32,
            hex_size,
            offset,
            background: minimap::rgb(theme::INSET_BG),
        },
        &cells,
    );
    // Capital markers (dark dot, white ring).
    for tile in &snapshot.tiles {
        if tile.is_country_capital {
            let pos = minimap::hex_to_px(tile.q, tile.r, hex_size) + offset;
            raster.draw_dot(
                pos,
                (hex_size * 0.3).max(1.5),
                [26, 26, 26],
                [255, 255, 255],
            );
        }
    }
    let image = images.add(raster.into_image());

    // Nation labels.
    let label_tiles: Vec<(i32, i32, &str)> = snapshot
        .tiles
        .iter()
        .filter(|t| t.terrain != "Sea" && !t.owner.is_empty())
        .map(|t| (t.q, t.r, t.visual_group_or_owner()))
        .collect();
    let labels = minimap::compute_nation_labels(&label_tiles, 3, hex_size, offset);

    // Legend: unique owners in tile order.
    let mut legend: Vec<(String, Color)> = Vec::new();
    for tile in &snapshot.tiles {
        if tile.owner.is_empty() || legend.iter().any(|(name, _)| *name == tile.owner) {
            continue;
        }
        let nation = theme::nation_color(&tile.owner_color);
        let color = if tile.is_incorporated_minor {
            theme::incorporated_tint(nation)
        } else {
            theme::political_tint(nation)
        };
        legend.push((tile.owner.clone(), color));
    }

    commands.entity(handles.content).with_children(|body| {
        body.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|row| {
            // Map image with absolutely-positioned label overlays.
            row.spawn((
                Node {
                    width: Val::Px(POLITICAL_MAP_W),
                    height: Val::Px(POLITICAL_MAP_H),
                    flex_shrink: 0.0,
                    // Label overlays near the edge must not spill out.
                    overflow: Overflow::clip(),
                    ..default()
                },
                ImageNode::new(image),
            ))
            .with_children(|map| {
                for label in &labels {
                    let font_size = ((label.size as f32).sqrt() * 2.4).clamp(10.0, 22.0);
                    map_label(map, theme, &label.name.to_uppercase(), label.pos, font_size);
                }
            });
            // Nation legend.
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                max_height: Val::Px(POLITICAL_MAP_H),
                overflow: Overflow::scroll_y(),
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Nations"),
                    theme.font_bold(12.0),
                    TextColor(theme::TEXT_DIM),
                ));
                for (name, color) in &legend {
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|entry| {
                            entry.spawn((
                                Node {
                                    width: Val::Px(12.0),
                                    height: Val::Px(12.0),
                                    border: UiRect::all(Val::Px(1.0)),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                BackgroundColor(*color),
                                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                            ));
                            entry.spawn((
                                Text::new(name.clone()),
                                theme.font(12.0),
                                TextColor(theme::TEXT),
                            ));
                        });
                }
            });
        });
    });
}

/// Centered map label with a 1px shadow for legibility (Bevy text has no
/// stroke; the web strokes the canvas text instead).
pub fn map_label(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    text: &str,
    pos: Vec2,
    font_size: f32,
) {
    const BOX_W: f32 = 240.0;
    const BOX_H: f32 = 28.0;
    // Shadow first, label second — later siblings render on top.
    for (dx, dy, color) in [
        (1.0, 1.0, Color::srgba(0.0, 0.0, 0.0, 0.55)),
        (0.0, 0.0, Color::srgba(1.0, 1.0, 1.0, 0.9)),
    ] {
        parent
            .spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(pos.x - BOX_W / 2.0 + dx),
                top: Val::Px(pos.y - BOX_H / 2.0 + dy),
                width: Val::Px(BOX_W),
                height: Val::Px(BOX_H),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|anchor| {
                anchor.spawn((
                    Text::new(text.to_string()),
                    theme.font_bold(font_size),
                    TextColor(color),
                ));
            });
    }
}
