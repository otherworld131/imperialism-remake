//! Shared helpers for the game screens (M7+): icon spawning, name
//! formatting, section headers, and the full-screen root scaffold.

use bevy::prelude::*;

use crate::map::icons::IconAssets;
use crate::map::picking::PickingBlocker;
use crate::theme::{self, Theme};

/// `1234567` → `"1,234,567"` (web `toLocaleString` parity).
pub fn fmt_thousands(n: i64) -> String {
    let negative = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if negative {
        out.push('-');
    }
    let first_group = digits.len() % 3;
    for (i, c) in digits.chars().enumerate() {
        if i != 0 && (i + 3 - first_group).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `"RifleInfantry"` → `"Rifle Infantry"`.
pub fn split_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Units-group icon for an army category (web `UNIT_EMOJI` parity).
pub fn unit_icon_name(category: &str) -> &'static str {
    match category {
        "Infantry" => "Infantry",
        "Cavalry" => "Cavalry",
        "Artillery" => "Artillery",
        "Special" => "Special",
        _ => "Army",
    }
}

pub fn spawn_icon(
    parent: &mut ChildSpawnerCommands,
    icons: Option<&IconAssets>,
    group: &str,
    name: &str,
    size: f32,
) {
    if let Some(image) = icons.and_then(|i| i.get(group, name)) {
        parent.spawn((
            Node {
                width: Val::Px(size),
                height: Val::Px(size),
                flex_shrink: 0.0,
                ..default()
            },
            ImageNode::new(image),
        ));
    }
}

/// CC-1 inset container: logical groups (a production chain, a warehouse
/// section, an army list) sit inside a visible panel instead of floating as
/// bare text on the screen background.
pub fn inset_panel() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            padding: UiRect::all(Val::Px(9.0)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            row_gap: Val::Px(3.0),
            margin: UiRect::bottom(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(theme::INSET_BG),
        BorderColor::all(theme::BORDER),
    )
}

/// Section header with a hairline top border (web `Section` parity).
pub fn section_title(parent: &mut ChildSpawnerCommands, theme: &Theme, label: &str) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                border: UiRect::top(Val::Px(1.0)),
                padding: UiRect::top(Val::Px(6.0)),
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
            BorderColor::all(theme::BORDER),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new(label.to_string()),
                theme.font_bold(14.0),
                TextColor(theme::GOLD),
            ));
        });
}

/// Icon + label row (commodity rows, cost lines, …).
pub fn icon_label(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    group: &str,
    icon_name: &str,
    text: &str,
    font_size: f32,
    color: Color,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|row| {
            spawn_icon(row, icons, group, icon_name, font_size + 2.0);
            row.spawn((
                Text::new(text.to_string()),
                theme.font(font_size),
                TextColor(color),
            ));
        });
}

/// Full-screen overlay root below the top bar: solid background, blocks
/// map picking, and despawns with the marker on screen exit.
pub fn full_screen_root(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(44.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            GlobalZIndex(50),
            Interaction::default(),
            PickingBlocker,
        ))
        .id()
}
