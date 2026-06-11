//! Debug widget gallery, enabled with `WIDGET_GALLERY=1`. Overlays the map
//! with a scrollable panel exercising every widget in the kit; interactions
//! are echoed as toasts so commit semantics are visible at a glance.

use bevy::prelude::*;
use std::sync::Arc;

use crate::map::picking::PickingBlocker;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ColumnSpec, button::ButtonProps, checkbox::CheckboxProps, dropdown::DropdownProps,
    dropdown::MultiDropdownProps, modal::ModalProps, progress::ProgressProps, scroll::ScrollProps,
    slider::SliderProps, table::TableProps, text_input::TextInputProps,
};

#[derive(Component)]
pub struct OpenModalButton;

#[derive(Component)]
pub struct ToastInfoButton;

#[derive(Component)]
pub struct ToastErrorButton;

#[derive(Component)]
pub struct BoundProgress;

#[derive(Component)]
pub struct BoundSlider;

pub fn setup_gallery(mut commands: Commands, theme: Res<Theme>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(44.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(460.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                border: UiRect::left(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID.with_alpha(0.97)),
            BorderColor::all(theme::BORDER),
            GlobalZIndex(80),
            Interaction::default(),
            PickingBlocker,
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Widget Gallery"),
                theme.font_blackletter(22.0),
                TextColor(theme::GOLD),
            ));
            panel.spawn((
                Text::new("WIDGET_GALLERY=1 — kit smoke screen"),
                theme.font_italic(12.0),
                TextColor(theme::TEXT_DIM),
            ));

            let scroll = widgets::spawn_scroll_area(
                panel,
                &theme,
                ScrollProps {
                    flex_grow: 1.0,
                    ..default()
                },
            );
            let mut commands = panel.commands();
            commands.entity(scroll.content).with_children(|content| {
                fill_gallery(content, &theme);
            });
        });
}

fn section(parent: &mut ChildSpawnerCommands, theme: &Theme, title: &str) {
    parent.spawn((
        Text::new(title),
        theme.font_bold(14.0),
        TextColor(theme::GOLD),
        Node {
            margin: UiRect::top(Val::Px(14.0)),
            ..default()
        },
    ));
}

fn row(parent: &mut ChildSpawnerCommands) -> Entity {
    parent
        .spawn((Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            flex_wrap: FlexWrap::Wrap,
            row_gap: Val::Px(6.0),
            margin: UiRect::vertical(Val::Px(4.0)),
            ..default()
        },))
        .id()
}

fn fill_gallery(content: &mut ChildSpawnerCommands, theme: &Theme) {
    // ── Buttons ──
    section(content, theme, "Buttons");
    let buttons_row = row(content);
    content
        .commands()
        .entity(buttons_row)
        .with_children(|buttons| {
            let normal = widgets::spawn_button(buttons, theme, ButtonProps::label("Normal"));
            buttons
                .commands()
                .entity(normal)
                .insert(widgets::TooltipText("A plain kit button.".into()));
            widgets::spawn_button(
                buttons,
                theme,
                ButtonProps {
                    label: "Disabled".into(),
                    enabled: false,
                    ..default()
                },
            );
            let modal = widgets::spawn_button(buttons, theme, ButtonProps::label("Open modal"));
            buttons.commands().entity(modal).insert(OpenModalButton);
            let info = widgets::spawn_button(buttons, theme, ButtonProps::label("Toast"));
            buttons.commands().entity(info).insert(ToastInfoButton);
            let error = widgets::spawn_button(buttons, theme, ButtonProps::label("Toast error"));
            buttons.commands().entity(error).insert(ToastErrorButton);
        });

    // ── Checkboxes & radio ──
    section(content, theme, "Checkbox & radio group");
    let checks_row = row(content);
    content
        .commands()
        .entity(checks_row)
        .with_children(|checks| {
            widgets::spawn_checkbox(
                checks,
                theme,
                CheckboxProps {
                    label: "Escort convoys".into(),
                    checked: true,
                    ..default()
                },
            );
            widgets::spawn_checkbox(
                checks,
                theme,
                CheckboxProps {
                    label: "Auto-prospect".into(),
                    ..default()
                },
            );
            widgets::spawn_checkbox(
                checks,
                theme,
                CheckboxProps {
                    label: "Disabled".into(),
                    enabled: false,
                    ..default()
                },
            );
        });
    widgets::spawn_radio_group(
        content,
        theme,
        &["Peace footing", "War footing", "Total mobilization"],
        0,
    );

    // ── Sliders ──
    section(content, theme, "Sliders (commit on release)");
    widgets::spawn_slider(
        content,
        theme,
        SliderProps {
            min: 0.0,
            max: 100.0,
            step: 5.0,
            value: 40.0,
            ..default()
        },
    );
    let money = widgets::spawn_slider(
        content,
        theme,
        SliderProps {
            min: 0.0,
            max: 50_000.0,
            step: 2_500.0,
            value: 10_000.0,
            format: Some(Arc::new(|v| format!("${v:.0}"))),
            ..default()
        },
    );
    content.commands().entity(money).insert(BoundSlider);
    content.spawn((
        Text::new("Unlimited notch (one step past max ⇒ ∞):"),
        theme.font_italic(12.0),
        TextColor(theme::TEXT_DIM),
    ));
    widgets::spawn_slider(
        content,
        theme,
        SliderProps {
            min: 0.0,
            max: 50.0,
            step: 5.0,
            value: 25.0,
            unlimited: true,
            ..default()
        },
    );

    // ── Dropdowns ──
    section(content, theme, "Dropdowns");
    let dropdown_row = row(content);
    content
        .commands()
        .entity(dropdown_row)
        .with_children(|drops| {
            widgets::spawn_dropdown(
                drops,
                theme,
                DropdownProps {
                    options: vec![
                        "Hapsburg Empire".into(),
                        "France".into(),
                        "Great Britain".into(),
                        "Prussia".into(),
                        "Russia".into(),
                        "Sardinia".into(),
                        "Sweden".into(),
                    ],
                    selected: 1,
                    width: Val::Px(190.0),
                },
            );
            widgets::spawn_multi_dropdown(
                drops,
                theme,
                MultiDropdownProps {
                    label: "Filters".into(),
                    options: vec![
                        "Option A".into(),
                        "Option B".into(),
                        "Option C".into(),
                        "Option D".into(),
                        "Option E".into(),
                    ],
                    selected: vec![true, true, false, false, false],
                    width: Val::Px(190.0),
                },
            );
        });

    // ── Tabs ──
    section(content, theme, "Tabs");
    let tabs = widgets::spawn_tabs(content, theme, &["Industry", "Trade", "Diplomacy"], 0);
    let mut commands = content.commands();
    let bodies = [
        "Factories hum along the Rhine.",
        "Merchantmen await a bid.",
        "The council convenes in spring.",
    ];
    for (panel, body) in tabs.panels.iter().zip(bodies) {
        let theme_font = theme.font(12.5);
        commands.entity(*panel).with_children(|tab| {
            tab.spawn((Text::new(body), theme_font, TextColor(theme::TEXT)));
        });
    }

    // ── Table ──
    section(content, theme, "Table (click headers to sort)");
    widgets::spawn_table(
        content,
        theme,
        TableProps {
            columns: vec![
                ColumnSpec::new("Nation", 2.0),
                ColumnSpec::new("Gold", 1.0),
                ColumnSpec::new("Regiments", 1.0),
            ],
            sortable: true,
            rows: vec![
                vec!["France".into(), "182500".into(), "14".into()],
                vec!["Prussia".into(), "97250".into(), "18".into()],
                vec!["Great Britain".into(), "240000".into(), "9".into()],
                vec!["Russia".into(), "61000".into(), "22".into()],
                vec!["Sardinia".into(), "33500".into(), "5".into()],
                vec!["Sweden".into(), "48750".into(), "7".into()],
            ],
            cell_builder: None,
        },
    );

    // ── Progress ──
    section(content, theme, "Progress");
    widgets::spawn_progress(
        content,
        theme,
        ProgressProps {
            fraction: 0.3,
            ..default()
        },
    );
    content.spawn((
        Text::new("Bound to the $ slider above:"),
        theme.font_italic(12.0),
        TextColor(theme::TEXT_DIM),
    ));
    let bound = widgets::spawn_progress(
        content,
        theme,
        ProgressProps {
            fraction: 0.2,
            ..default()
        },
    );
    content.commands().entity(bound).insert(BoundProgress);

    // ── Text input ──
    section(content, theme, "Text input (click to focus, max 24 chars)");
    widgets::spawn_text_input(
        content,
        theme,
        TextInputProps {
            max_len: 24,
            value: "Devron".into(),
            ..default()
        },
    );

    // ── Tooltip ──
    section(content, theme, "Tooltip");
    content.spawn((
        Text::new("Hover me for half a second."),
        theme.font(13.0),
        TextColor(theme::TEXT),
        Interaction::default(),
        widgets::TooltipText("Tooltips follow the cursor and clamp to the window.".into()),
    ));
}

/// Echo widget interactions as toasts and drive the bound progress bar.
pub fn gallery_interactions(
    mut commands: Commands,
    theme: Res<Theme>,
    mut stack: ResMut<widgets::ModalStack>,
    mut activations: MessageReader<widgets::ButtonActivated>,
    mut commits: MessageReader<widgets::SliderCommitted>,
    mut dropdowns: MessageReader<widgets::DropdownChanged>,
    mut multis: MessageReader<widgets::MultiDropdownChanged>,
    mut radios: MessageReader<widgets::RadioSelected>,
    mut toasts: MessageWriter<widgets::Toast>,
    open_modal_buttons: Query<(), With<OpenModalButton>>,
    info_buttons: Query<(), With<ToastInfoButton>>,
    error_buttons: Query<(), With<ToastErrorButton>>,
    bound_sliders: Query<&widgets::UiSlider, With<BoundSlider>>,
    mut bound_progress: Query<&mut widgets::UiProgress, With<BoundProgress>>,
) {
    for widgets::ButtonActivated(entity) in activations.read() {
        if open_modal_buttons.contains(*entity) {
            let depth = stack.len() + 1;
            let handles = widgets::open_modal(
                &mut commands,
                &mut stack,
                &theme,
                ModalProps {
                    title: format!("Royal Decree #{depth}"),
                    ..default()
                },
            );
            let body_font = theme.font(13.0);
            commands.entity(handles.content).with_children(|body| {
                body.spawn((
                    Text::new(
                        "Esc or ✕ closes the top modal. Modals stack: \
                         open another from the gallery behind this one.",
                    ),
                    body_font,
                    TextColor(theme::TEXT),
                ));
            });
        } else if info_buttons.contains(*entity) {
            toasts.write(widgets::Toast::info("Dispatch received from the capital."));
        } else if error_buttons.contains(*entity) {
            toasts.write(widgets::Toast::error(
                "The treasury cannot cover that order.",
            ));
        }
    }
    for commit in commits.read() {
        let value = if commit.unlimited {
            "∞ (u32::MAX)".to_string()
        } else {
            format!("{}", commit.as_u32())
        };
        toasts.write(widgets::Toast::success(format!(
            "Slider committed: {value}"
        )));
        if let Ok(slider) = bound_sliders.get(commit.entity) {
            let fraction = if commit.unlimited {
                1.0
            } else {
                (commit.value - slider.min) / (slider.max - slider.min).max(f32::MIN_POSITIVE)
            };
            for mut progress in &mut bound_progress {
                progress.fraction = fraction;
            }
        }
    }
    for change in dropdowns.read() {
        toasts.write(widgets::Toast::info(format!(
            "Dropdown picked option {}",
            change.index + 1
        )));
    }
    for change in multis.read() {
        let count = change.selected.iter().filter(|s| **s).count();
        toasts.write(widgets::Toast::info(format!(
            "Filter selection: {count}/{}",
            change.selected.len()
        )));
    }
    for change in radios.read() {
        toasts.write(widgets::Toast::info(format!(
            "Footing changed to option {}",
            change.index + 1
        )));
    }
}
