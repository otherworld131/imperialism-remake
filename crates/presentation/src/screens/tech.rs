//! Technology screen (F6): full-screen overlay mirroring the web
//! `TechScreen` — pending-research banner with Cancel, the available-tech
//! table (queued row highlighted, $cost buttons gated by treasury /
//! pending / observer), and dimmed researched + historic rows. Queueing a
//! tech only marks pending research; the turn processor deducts the cost
//! and applies the effects at end turn.

use bevy::prelude::*;

use crate::game::commands::GameCommand;
use crate::game::resources::{GameMeta, TurnInfo, ViewModels};
use crate::screens::common::{fmt_thousands, full_screen_root};
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ScrollProps, TooltipText};

const QUEUED_BG: Color = Color::srgba(218.0 / 255.0, 165.0 / 255.0, 32.0 / 255.0, 0.06);
const DESC_BLUE: Color = Color::srgb_u8(0x8a, 0x9a, 0xaa);
const DIM: Color = Color::srgb_u8(0x9a, 0x9a, 0x9a);
const RESEARCHED_GREEN: Color = Color::srgb_u8(0x5a, 0x7a, 0x5a);

#[derive(Component)]
pub struct TechRoot;

#[derive(Component)]
pub struct TechContent;

#[derive(Component)]
pub struct TechQueueButton(pub String);

#[derive(Component)]
pub struct TechCancelButton;

#[derive(Component)]
pub struct TechCloseButton;

pub fn enter_tech(mut commands: Commands, theme: Res<Theme>) {
    let root = full_screen_root(&mut commands);
    commands.entity(root).insert(TechRoot);
    commands.entity(root).with_children(|panel| {
        let scroll = widgets::spawn_scroll_area(
            panel,
            &theme,
            ScrollProps {
                flex_grow: 1.0,
                ..default()
            },
        );
        panel.commands().entity(scroll.content).insert(TechContent);
    });
}

pub fn exit_tech(mut commands: Commands, roots: Query<Entity, With<TechRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

pub fn update_tech(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    turn_info: Res<TurnInfo>,
    theme: Res<Theme>,
    mut commands: Commands,
    sections: Query<Entity, With<TechContent>>,
    added: Query<(), Added<TechContent>>,
) {
    if !vms.is_changed() && added.is_empty() {
        return;
    }
    let Ok(section) = sections.single() else {
        return;
    };
    commands.entity(section).despawn_children();
    let Some(tech) = vms.tech.as_ref() else {
        return;
    };
    let observer = meta.observer;
    let year = turn_info.year;

    commands.entity(section).with_children(|content| {
        // Header.
        content
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::bottom(Val::Px(8.0)),
                    border: UiRect::bottom(Val::Px(2.0)),
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|header| {
                header.spawn((
                    Text::new(format!("Technology — {year}")),
                    theme.font_bold(19.0),
                    TextColor(theme::GOLD),
                ));
                header.spawn((
                    Text::new(format!("Treasury: ${}", fmt_thousands(tech.treasury))),
                    theme.font(13.0),
                    TextColor(theme::TEXT_DIM),
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
                header.commands().entity(close).insert(TechCloseButton);
            });

        // Pending-research banner.
        if let Some(pending) = tech.pending.as_ref() {
            content
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(16.0),
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        margin: UiRect::bottom(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(
                        218.0 / 255.0,
                        165.0 / 255.0,
                        32.0 / 255.0,
                        0.12,
                    )),
                    BorderColor::all(Color::srgba(
                        218.0 / 255.0,
                        165.0 / 255.0,
                        32.0 / 255.0,
                        0.4,
                    )),
                ))
                .with_children(|banner| {
                    banner
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            flex_wrap: FlexWrap::Wrap,
                            ..default()
                        })
                        .with_children(|line| {
                            line.spawn((
                                Text::new("Queued:"),
                                theme.font(13.0),
                                TextColor(theme::TEXT),
                            ));
                            line.spawn((
                                Text::new(pending.name.clone()),
                                theme.font_bold(13.0),
                                TextColor(theme::TEXT),
                            ));
                            if pending.cost > 0 {
                                line.spawn((
                                    Text::new(format!("(${})", fmt_thousands(pending.cost))),
                                    theme.font_bold(13.0),
                                    TextColor(Color::srgb_u8(0xff, 0xd9, 0x00)),
                                ));
                            }
                            if !pending.description.is_empty() {
                                line.spawn((
                                    Text::new(format!("— {}", pending.description)),
                                    theme.font_italic(12.0),
                                    TextColor(Color::srgb_u8(0xaa, 0xaa, 0xaa)),
                                ));
                            }
                            line.spawn((
                                Text::new("researched at end of turn"),
                                theme.font(12.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                        });
                    if !observer {
                        let cancel = widgets::spawn_button(
                            banner,
                            &theme,
                            ButtonProps {
                                label: "Cancel".into(),
                                font_size: 13.0,
                                ..default()
                            },
                        );
                        banner.commands().entity(cancel).insert(TechCancelButton);
                    }
                });
        }

        // Table header.
        content
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    padding: UiRect::vertical(Val::Px(6.0)),
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(theme::BORDER),
            ))
            .with_children(|header| {
                for (label, basis, grow, right) in [
                    ("TECHNOLOGY", 240.0, 0.0, false),
                    ("EFFECT", 0.0, 1.0, false),
                    ("PURCHASE / STATUS", 170.0, 0.0, true),
                ] {
                    header
                        .spawn(Node {
                            flex_basis: Val::Px(basis),
                            flex_grow: grow,
                            flex_shrink: 0.0,
                            justify_content: if right {
                                JustifyContent::FlexEnd
                            } else {
                                JustifyContent::FlexStart
                            },
                            ..default()
                        })
                        .with_children(|cell| {
                            cell.spawn((
                                Text::new(label),
                                theme.font(11.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                        });
                }
            });

        let researched_years: std::collections::HashMap<u32, i32> =
            tech.researched.iter().map(|t| (t.id, t.year)).collect();
        let available_ids: std::collections::HashSet<u32> =
            tech.available.iter().map(|t| t.id).collect();

        // The full 1815–1915 timeline: adopted, available, and future techs
        // grouped by decade, so the screen is a planning view. Older saves
        // without the timeline field fall back to the available list.
        let timeline: Vec<&crate::game::vm::TechAvailableVm> = if tech.timeline.is_empty() {
            tech.available.iter().collect()
        } else {
            tech.timeline.iter().collect()
        };

        if timeline.is_empty() {
            content.spawn((
                Text::new("No technologies in this scenario."),
                theme.font_italic(12.5),
                TextColor(DIM),
                Node {
                    margin: UiRect::all(Val::Px(24.0)),
                    align_self: AlignSelf::Center,
                    ..default()
                },
            ));
        }

        let mut last_decade: Option<i32> = None;
        for entry in timeline {
            let decade = entry.earliest_year / 10 * 10;
            if last_decade != Some(decade) {
                last_decade = Some(decade);
                content.spawn((
                    Text::new(format!("{decade}s")),
                    theme.font_bold(13.0),
                    TextColor(theme::GOLD),
                    Node {
                        margin: UiRect::new(
                            Val::Px(0.0),
                            Val::Px(0.0),
                            Val::Px(10.0),
                            Val::Px(2.0),
                        ),
                        ..default()
                    },
                ));
            }

            let researched_year = researched_years.get(&entry.id).copied();
            let is_available = available_ids.contains(&entry.id);
            let is_queued = tech
                .pending
                .as_ref()
                .is_some_and(|pending| pending.id == entry.id);
            let can_afford = tech.treasury >= entry.cost;
            let year_range = (entry.latest_year < 9999).then(|| {
                if entry.earliest_year == entry.latest_year {
                    entry.earliest_year.to_string()
                } else {
                    format!("{}–{}", entry.earliest_year, entry.latest_year)
                }
            });
            let locked = researched_year.is_none() && !is_available;

            tech_row(
                content,
                &theme,
                &entry.name,
                year_range.as_deref(),
                &entry.description,
                if is_queued { Some(QUEUED_BG) } else { None },
                researched_year.is_some() || locked,
                |cell, theme| {
                    if let Some(year) = researched_year {
                        researched_chip(cell, theme, year);
                    } else if locked {
                        // Future/locked: availability year + cost, grayed.
                        let label = if entry.cost > 0 {
                            format!(
                                "from {} · ${}",
                                entry.earliest_year,
                                fmt_thousands(entry.cost)
                            )
                        } else {
                            format!("from {}", entry.earliest_year)
                        };
                        cell.spawn((
                            Text::new(label),
                            theme.font(12.0),
                            TextColor(DIM),
                            TooltipText(format!(
                                "Not yet available — unlocks from {} once its \
                                 prerequisites are researched",
                                entry.earliest_year
                            )),
                        ));
                    } else if is_queued {
                        cell.spawn((
                            Text::new("Queued ✓"),
                            theme.font_italic(12.0),
                            TextColor(theme::GOLD),
                        ));
                    } else {
                        let enabled = !observer && can_afford && tech.pending.is_none();
                        let button = widgets::spawn_button(
                            cell,
                            theme,
                            ButtonProps {
                                label: if entry.cost > 0 {
                                    format!("Adopt (${})", fmt_thousands(entry.cost))
                                } else {
                                    "Adopt (free)".into()
                                },
                                font_size: 13.0,
                                enabled,
                                ..default()
                            },
                        );
                        let mut commands = cell.commands();
                        let mut button_commands = commands.entity(button);
                        button_commands.insert(TechQueueButton(entry.name.clone()));
                        if !can_afford {
                            button_commands.insert(TooltipText(format!(
                                "Insufficient funds (need ${})",
                                fmt_thousands(entry.cost)
                            )));
                        } else if tech.pending.is_some() {
                            button_commands
                                .insert(TooltipText("Cancel the current queued tech first".into()));
                        }
                        if enabled {
                            // Green purchase border (web `purchaseBtn`).
                            button_commands
                                .insert(BorderColor::all(Color::srgb_u8(0x4a, 0x70, 0x30)));
                        }
                    }
                },
            );
        }
    });
}

/// `✓ {year}` chip on a researched row.
fn researched_chip(cell: &mut ChildSpawnerCommands, theme: &Theme, year: i32) {
    cell.spawn((
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            ..default()
        },
        BorderColor::all(Color::srgb_u8(0x3a, 0x5a, 0x3a)),
    ))
    .with_children(|chip| {
        chip.spawn((
            Text::new(if year > 0 {
                format!("✓ {year}")
            } else {
                "✓ Researched".into()
            }),
            theme.font(12.0),
            TextColor(RESEARCHED_GREEN),
        ));
    });
}

/// One table row: name (+ optional year range), description, action cell.
fn tech_row(
    content: &mut ChildSpawnerCommands,
    theme: &Theme,
    name: &str,
    year_range: Option<&str>,
    description: &str,
    background: Option<Color>,
    dimmed: bool,
    action: impl FnOnce(&mut ChildSpawnerCommands, &Theme),
) {
    content
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::vertical(Val::Px(6.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb_u8(0x22, 0x22, 0x33)),
            BackgroundColor(background.unwrap_or(Color::NONE)),
        ))
        .with_children(|row| {
            row.spawn(Node {
                flex_basis: Val::Px(240.0),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|cell| {
                cell.spawn((
                    Text::new(name),
                    theme.font(13.5),
                    TextColor(if dimmed { DIM } else { theme::TEXT }),
                ));
                if let Some(range) = year_range {
                    cell.spawn((
                        Text::new(range),
                        theme.font(11.0),
                        TextColor(Color::srgb_u8(0x66, 0x66, 0x66)),
                    ));
                }
            });
            row.spawn(Node {
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                ..default()
            })
            .with_children(|cell| {
                cell.spawn((
                    Text::new(description),
                    theme.font_italic(12.0),
                    TextColor(if dimmed { DIM } else { DESC_BLUE }),
                ));
            });
            row.spawn(Node {
                flex_basis: Val::Px(170.0),
                flex_shrink: 0.0,
                justify_content: JustifyContent::FlexEnd,
                ..default()
            })
            .with_children(|cell| {
                action(cell, theme);
            });
        });
}

pub fn handle_tech_buttons(
    mut activations: MessageReader<ButtonActivated>,
    queue_buttons: Query<&TechQueueButton>,
    cancel_buttons: Query<(), With<TechCancelButton>>,
    close_buttons: Query<(), With<TechCloseButton>>,
    mut next_screen: ResMut<NextState<crate::state::Screen>>,
    mut out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(queue) = queue_buttons.get(*entity) {
            out.write(GameCommand::QueueTechResearch {
                name: queue.0.clone(),
            });
        } else if cancel_buttons.contains(*entity) {
            out.write(GameCommand::CancelTechResearch);
        } else if close_buttons.contains(*entity) {
            next_screen.set(crate::state::Screen::Map);
        }
    }
}
