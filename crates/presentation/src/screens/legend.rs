//! Legend screen (F10, web `LegendScreen`): static reference for terrain
//! colors, resource icons, civilian types, infrastructure symbols, military
//! unit categories, diplomatic colors, the strength/relationship gradients,
//! and every nation's flag with its government title.

use bevy::prelude::*;

use crate::game::resources::ViewModels;
use crate::game::vm::NationInfoVm;
use crate::map::icons::IconAssets;
use crate::screens::common::{full_screen_root, spawn_icon};
use crate::screens::ledger::FlagCache;
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ScrollProps};

const ITEM_DESC: Color = Color::srgb_u8(0x99, 0x99, 0x99);
const CARD_W: f32 = 300.0;

const TERRAIN_LEGEND: [(&str, &str); 8] = [
    ("Grassland", "Fertile plains for farming"),
    ("Hills", "Elevated terrain, +30% defense"),
    ("Forest", "Timber source, +20% defense"),
    ("Mountain", "Mineral-rich, +50% defense"),
    ("Desert", "Arid terrain, limited use"),
    ("Swamp", "Difficult terrain, +15% defense"),
    ("Tundra", "Cold terrain, limited farming"),
    ("Sea", "Naval zones for trade and combat"),
];

const RESOURCE_LEGEND: [(&str, &str); 12] = [
    ("Grain", "Food staple, farmed on grassland"),
    ("Fruit", "Food resource, farmed on grassland"),
    ("Cotton", "Textile raw material"),
    ("Wool", "Textile raw material from ranching"),
    ("Timber", "Wood from forests, makes lumber"),
    ("Livestock", "Food resource from ranching"),
    ("Horses", "Required for cavalry units"),
    ("Coal", "Industrial fuel, mined in mountains/hills"),
    ("Iron", "Makes steel, mined in mountains/hills"),
    ("Gold", "Monetary resource, high value"),
    ("Gems", "Precious stones, very high value"),
    ("Oil", "Late-game industrial resource"),
];

const CIVILIAN_LEGEND: [(&str, &str); 7] = [
    ("Farmer", "Improves grassland for grain/fruit/cotton"),
    (
        "Miner",
        "Improves mountain/hill tiles for coal/iron/gold/gems",
    ),
    ("Engineer", "Builds railroads and infrastructure"),
    ("Forester", "Improves forest tiles for timber"),
    ("Rancher", "Improves grassland for wool/livestock/horses"),
    ("Driller", "Extracts oil from desert/swamp tiles"),
    ("Prospector", "Reveals hidden resources on tiles"),
];

/// `(icon name, display name, description)`. `Capitol` doubles as the
/// province-capital marker sprite.
const INFRASTRUCTURE_LEGEND: [(&str, &str, &str); 6] = [
    ("Capital", "Capital", "Nation capital (gold star)"),
    ("Capitol", "Province Capital", "Province center"),
    ("Railroad", "Railroad", "Transport network for resources"),
    ("Depot", "Depot", "Railroad junction point"),
    ("Port", "Port", "Enables naval trade and transport"),
    ("Fort", "Fort", "Defensive fortification (L1-L3)"),
];

const UNIT_LEGEND: [(&str, &str); 4] = [
    (
        "Infantry",
        "Militia • Regulars • Grenadiers • Rifle Infantry • Guards • Sharpshooters • Modern \
         Infantry • Machine Gunners • Rangers",
    ),
    (
        "Cavalry",
        "Cuirassiers • Scouts • Carbine Cavalry • Armour • Mechanised",
    ),
    (
        "Artillery",
        "Light Artillery • Standard Artillery • Field Artillery • Siege Artillery • Railroad \
         Gun • Mobile Artillery",
    ),
    ("Special", "Sapper • General"),
];

#[derive(Component)]
pub struct LegendRoot;

#[derive(Component)]
pub struct LegendCloseButton;

pub fn exit_legend(mut commands: Commands, roots: Query<Entity, With<LegendRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

pub fn handle_legend_buttons(
    mut activations: MessageReader<ButtonActivated>,
    closes: Query<(), With<LegendCloseButton>>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    for ButtonActivated(entity) in activations.read() {
        if closes.contains(*entity) {
            next_screen.set(Screen::Map);
        }
    }
}

pub fn enter_legend(
    mut commands: Commands,
    theme: Res<Theme>,
    vms: Res<ViewModels>,
    flags: Res<FlagCache>,
    icons: Option<Res<IconAssets>>,
) {
    let icons = icons.as_deref();
    let root = full_screen_root(&mut commands);
    commands.entity(root).insert(LegendRoot);
    commands.entity(root).with_children(|panel| {
        // Header.
        panel
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
                    Text::new("Legend"),
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
                header.commands().entity(close).insert(LegendCloseButton);
            });

        let scroll = widgets::spawn_scroll_area(
            panel,
            &theme,
            ScrollProps {
                flex_grow: 1.0,
                ..default()
            },
        );
        let mut panel_commands = panel.commands();
        panel_commands.entity(scroll.content).with_children(|body| {
            // Terrain.
            section(body, &theme, "Terrain", |grid, theme| {
                for (name, desc) in TERRAIN_LEGEND {
                    legend_item(grid, theme, name, desc, |slot, _| {
                        slot.spawn((
                            Node {
                                width: Val::Px(24.0),
                                height: Val::Px(24.0),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            BackgroundColor(theme::terrain_color(name)),
                            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.15)),
                        ));
                    });
                }
            });

            // Resources.
            section(body, &theme, "Resources", |grid, theme| {
                for (name, desc) in RESOURCE_LEGEND {
                    legend_item(grid, theme, name, desc, |slot, _| {
                        spawn_icon(slot, icons, "commodities", name, 22.0);
                    });
                }
            });

            // Civilians.
            section(body, &theme, "Civilians", |grid, theme| {
                for (name, desc) in CIVILIAN_LEGEND {
                    legend_item(grid, theme, name, desc, |slot, _| {
                        spawn_icon(slot, icons, "civilians", name, 22.0);
                    });
                }
            });

            // Infrastructure.
            section(body, &theme, "Infrastructure", |grid, theme| {
                for (icon, name, desc) in INFRASTRUCTURE_LEGEND {
                    legend_item(grid, theme, name, desc, |slot, _| {
                        spawn_icon(slot, icons, "infrastructure", icon, 22.0);
                    });
                }
            });

            // Military units.
            section(body, &theme, "Military Units", |grid, theme| {
                for (category, units) in UNIT_LEGEND {
                    grid.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        width: Val::Percent(100.0),
                        row_gap: Val::Px(2.0),
                        margin: UiRect::bottom(Val::Px(8.0)),
                        ..default()
                    })
                    .with_children(|block| {
                        block
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                ..default()
                            })
                            .with_children(|header| {
                                spawn_icon(header, icons, "units", category, 16.0);
                                header.spawn((
                                    Text::new(category.to_uppercase()),
                                    theme.font(12.0),
                                    TextColor(theme::GOLD),
                                ));
                            });
                        block.spawn((
                            Text::new(units),
                            theme.font(13.0),
                            TextColor(Color::srgb_u8(0xcc, 0xcc, 0xcc)),
                        ));
                    });
                }
            });

            // Diplomatic colors.
            section(body, &theme, "Diplomatic Map Mode", |grid, theme| {
                let entries = [
                    (theme::OVERLAY_SELF, "Self (your nation)"),
                    (theme::diplo_status_color("Alliance"), "Alliance"),
                    (theme::diplo_status_color("NAP"), "Non-Aggression Pact"),
                    (theme::diplo_status_color("At War"), "At War"),
                    (theme::diplo_status_color("Neutral"), "Neutral"),
                ];
                for (color, label) in entries {
                    legend_item(grid, theme, label, "", |slot, _| {
                        slot.spawn((
                            Node {
                                width: Val::Px(24.0),
                                height: Val::Px(24.0),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            BackgroundColor(color),
                            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.15)),
                        ));
                    });
                }
            });

            // Strength gradients.
            section(body, &theme, "Strength Map Modes", |column, theme| {
                gradient_row(
                    column,
                    theme,
                    "Military / Naval Strength (relative to average)",
                    "Weak",
                    "Strong",
                    theme::strength_color,
                );
                gradient_row(
                    column,
                    theme,
                    "Relationship Score",
                    "-100",
                    "+100",
                    theme::score_color,
                );
            });

            // Nations & flags.
            let great_powers: Vec<&NationInfoVm> =
                vms.nations.iter().filter(|n| n.is_great_power()).collect();
            let minors: Vec<&NationInfoVm> =
                vms.nations.iter().filter(|n| !n.is_great_power()).collect();
            if !great_powers.is_empty() || !minors.is_empty() {
                section(body, &theme, "Nations", |column, theme| {
                    for (title, nations, flag_w, flag_h) in [
                        ("GREAT POWERS", &great_powers, 150.0, 100.0),
                        ("MINOR NATIONS", &minors, 120.0, 80.0),
                    ] {
                        if nations.is_empty() {
                            continue;
                        }
                        column.spawn((
                            Text::new(title),
                            theme.font(12.0),
                            TextColor(theme::GOLD),
                            Node {
                                margin: UiRect::vertical(Val::Px(6.0)),
                                ..default()
                            },
                        ));
                        column
                            .spawn(Node {
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: Val::Px(16.0),
                                row_gap: Val::Px(16.0),
                                width: Val::Percent(100.0),
                                ..default()
                            })
                            .with_children(|grid| {
                                for nation in nations.iter() {
                                    flag_card(grid, theme, &flags, nation, flag_w, flag_h);
                                }
                            });
                    }
                });
            }
        });
    });
}

/// Section with gold title and a wrapping item grid.
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
                width: Val::Percent(100.0),
                padding: UiRect::bottom(Val::Px(14.0)),
                margin: UiRect::bottom(Val::Px(14.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(theme::BORDER),
        ))
        .with_children(|card| {
            card.spawn((
                Text::new(title.to_string()),
                theme.font_bold(17.0),
                TextColor(theme::GOLD),
                Node {
                    margin: UiRect::bottom(Val::Px(10.0)),
                    ..default()
                },
            ));
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(16.0),
                row_gap: Val::Px(6.0),
                width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|grid| {
                body(grid, theme);
            });
        });
}

/// Fixed-width card: symbol slot + name + description.
fn legend_item(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    name: &str,
    desc: &str,
    symbol: impl FnOnce(&mut ChildSpawnerCommands, &Theme),
) {
    parent
        .spawn(Node {
            width: Val::Px(CARD_W),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            padding: UiRect::vertical(Val::Px(4.0)),
            ..default()
        })
        .with_children(|item| {
            item.spawn(Node {
                width: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|slot| {
                symbol(slot, theme);
            });
            item.spawn(Node {
                flex_direction: FlexDirection::Column,
                min_width: Val::Px(0.0),
                ..default()
            })
            .with_children(|text| {
                text.spawn((
                    Text::new(name.to_string()),
                    theme.font_bold(13.0),
                    TextColor(theme::TEXT),
                ));
                if !desc.is_empty() {
                    text.spawn((
                        Text::new(desc.to_string()),
                        theme.font(12.0),
                        TextColor(ITEM_DESC),
                    ));
                }
            });
        });
}

/// Label + lo→hi gradient strip (20 steps, web linear-gradient parity).
fn gradient_row(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    label: &str,
    lo: &str,
    hi: &str,
    color_of: fn(f32) -> Color,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            row_gap: Val::Px(4.0),
            margin: UiRect::bottom(Val::Px(10.0)),
            ..default()
        })
        .with_children(|block| {
            block.spawn((
                Text::new(label.to_string()),
                theme.font(12.5),
                TextColor(ITEM_DESC),
            ));
            block
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    width: Val::Percent(100.0),
                    max_width: Val::Px(520.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(lo.to_string()),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                    ));
                    row.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1.0,
                        height: Val::Px(16.0),
                        ..default()
                    })
                    .with_children(|bar| {
                        const STEPS: usize = 20;
                        for i in 0..STEPS {
                            let score = -100.0 + 200.0 * i as f32 / (STEPS - 1) as f32;
                            bar.spawn((
                                Node {
                                    flex_grow: 1.0,
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(color_of(score)),
                            ));
                        }
                    });
                    row.spawn((
                        Text::new(hi.to_string()),
                        theme.font(12.0),
                        TextColor(theme::TEXT),
                    ));
                });
        });
}

/// Flag card: flag image, color dot, name, government title.
fn flag_card(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    flags: &FlagCache,
    nation: &NationInfoVm,
    flag_w: f32,
    flag_h: f32,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                width: Val::Px(flag_w + 22.0),
                ..default()
            },
            BorderColor::all(theme::BORDER),
        ))
        .with_children(|card| {
            if let Some(flag) = flags.get(nation.nation_id) {
                card.spawn((
                    Node {
                        width: Val::Px(flag_w),
                        height: Val::Px(flag_h),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ImageNode::new(flag),
                ));
            } else {
                card.spawn((
                    Node {
                        width: Val::Px(flag_w),
                        height: Val::Px(flag_h),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(theme::INSET_BG),
                ))
                .with_children(|placeholder| {
                    placeholder.spawn((
                        Text::new("No flag"),
                        theme.font_italic(11.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                });
            }
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|caption| {
                caption.spawn((
                    Node {
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::MAX,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(theme::nation_color(&nation.color)),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
                ));
                caption
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        min_width: Val::Px(0.0),
                        ..default()
                    })
                    .with_children(|text| {
                        text.spawn((
                            Text::new(nation.name.clone()),
                            theme.font_bold(13.0),
                            TextColor(theme::TEXT),
                        ));
                        if !nation.government_title.is_empty()
                            && nation.government_title != nation.name
                        {
                            text.spawn((
                                Text::new(nation.government_title.clone()),
                                theme.font(11.0),
                                TextColor(Color::srgb_u8(0x9a, 0x9a, 0x9a)),
                            ));
                        }
                    });
            });
        });
}
