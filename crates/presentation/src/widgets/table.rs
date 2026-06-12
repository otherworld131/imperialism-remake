//! Grid-based table with sortable header columns. No virtualization — meant
//! for the few-dozen-row panels (trade filters, province lists) of the game
//! screens.

use std::sync::Arc;

use bevy::picking::Pickable;
use bevy::prelude::*;

use super::button::{ButtonActivated, ButtonProps, spawn_button};
use crate::theme::{self, Theme};

/// Builds a custom cell; receives `(row, col, value)`. Defaults to plain text.
pub type CellBuilder =
    Arc<dyn Fn(&mut ChildSpawnerCommands, &Theme, usize, usize, &str) + Send + Sync>;

#[derive(Clone)]
pub struct ColumnSpec {
    pub header: String,
    /// Relative width in `fr` units.
    pub width: f32,
}

impl ColumnSpec {
    pub fn new(header: impl Into<String>, width: f32) -> Self {
        Self {
            header: header.into(),
            width,
        }
    }
}

pub struct TableProps {
    pub columns: Vec<ColumnSpec>,
    pub sortable: bool,
    pub rows: Vec<Vec<String>>,
    pub cell_builder: Option<CellBuilder>,
    /// Initial `(column index, ascending)` sort, e.g. to preserve sorting
    /// across a rebuild of the table.
    pub sort: Option<(usize, bool)>,
}

/// Table state. Mutate `rows` (or `sort`) and the body rebuilds.
#[derive(Component)]
pub struct UiTable {
    pub columns: Vec<ColumnSpec>,
    pub sortable: bool,
    pub rows: Vec<Vec<String>>,
    /// `(column index, ascending)`.
    pub sort: Option<(usize, bool)>,
    cell_builder: Option<CellBuilder>,
}

#[derive(Component)]
struct TableHeaderCell {
    table: Entity,
    col: usize,
}

#[derive(Component)]
struct TableBody(Entity);

pub fn spawn_table(parent: &mut ChildSpawnerCommands, theme: &Theme, props: TableProps) -> Entity {
    let template: Vec<RepeatedGridTrack> = props
        .columns
        .iter()
        .map(|c| RepeatedGridTrack::fr(1, c.width))
        .collect();
    let root = parent
        .spawn((
            UiTable {
                columns: props.columns.clone(),
                sortable: props.sortable,
                rows: props.rows,
                sort: props.sort,
                cell_builder: props.cell_builder,
            },
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(theme::BORDER),
        ))
        .id();
    let mut commands = parent.commands();
    commands.entity(root).with_children(|table| {
        // Header row.
        table
            .spawn((
                Node {
                    display: Display::Grid,
                    grid_template_columns: template.clone(),
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(theme::BORDER),
                BackgroundColor(theme::INSET_BG),
            ))
            .with_children(|header| {
                for (col, spec) in props.columns.iter().enumerate() {
                    if props.sortable {
                        let cell = spawn_button(
                            header,
                            theme,
                            ButtonProps {
                                label: spec.header.clone(),
                                font_size: 12.5,
                                flat: true,
                                ..default()
                            },
                        );
                        header
                            .commands()
                            .entity(cell)
                            .insert(TableHeaderCell { table: root, col });
                    } else {
                        header
                            .spawn((Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                ..default()
                            },))
                            .with_children(|cell| {
                                cell.spawn((
                                    Text::new(spec.header.clone()),
                                    theme.font_bold(12.5),
                                    TextColor(theme::GOLD),
                                    Pickable::IGNORE,
                                ));
                            });
                    }
                }
            });
        // Body grid, rebuilt by `rebuild_table_bodies`.
        table.spawn((
            TableBody(root),
            Node {
                display: Display::Grid,
                grid_template_columns: template,
                ..default()
            },
        ));
    });
    root
}

fn handle_header_clicks(
    mut activations: MessageReader<ButtonActivated>,
    headers: Query<&TableHeaderCell>,
    mut tables: Query<&mut UiTable>,
) {
    for ButtonActivated(entity) in activations.read() {
        let Ok(header) = headers.get(*entity) else {
            continue;
        };
        let Ok(mut table) = tables.get_mut(header.table) else {
            continue;
        };
        if !table.sortable {
            continue;
        }
        table.sort = match table.sort {
            Some((col, ascending)) if col == header.col => Some((col, !ascending)),
            _ => Some((header.col, true)),
        };
    }
}

/// Compare two cells numerically when both parse, lexicographically otherwise.
fn compare_cells(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.cmp(b),
    }
}

fn rebuild_table_bodies(
    theme: Res<Theme>,
    mut commands: Commands,
    tables: Query<(Entity, &UiTable), Changed<UiTable>>,
    bodies: Query<(Entity, &TableBody)>,
    headers: Query<(Entity, &TableHeaderCell, &Children)>,
    mut labels: Query<&mut Text>,
) {
    for (table_entity, table) in &tables {
        // Refresh sort indicators on header cells.
        for (_, header, children) in &headers {
            if header.table != table_entity {
                continue;
            }
            let spec_label = table
                .columns
                .get(header.col)
                .map_or(String::new(), |c| c.header.clone());
            let suffix = match table.sort {
                Some((col, true)) if col == header.col => " ▲",
                Some((col, false)) if col == header.col => " ▼",
                _ => "",
            };
            for child in children {
                if let Ok(mut text) = labels.get_mut(*child) {
                    let target = format!("{spec_label}{suffix}");
                    if **text != target {
                        **text = target;
                    }
                }
            }
        }

        let Some((body, _)) = bodies.iter().find(|(_, b)| b.0 == table_entity) else {
            continue;
        };

        // Sorted view of the rows.
        let mut order: Vec<usize> = (0..table.rows.len()).collect();
        if let Some((col, ascending)) = table.sort {
            order.sort_by(|&a, &b| {
                let left = table.rows[a].get(col).map_or("", String::as_str);
                let right = table.rows[b].get(col).map_or("", String::as_str);
                let ord = compare_cells(left, right);
                if ascending { ord } else { ord.reverse() }
            });
        }

        let mut body_commands = commands.entity(body);
        body_commands.despawn_related::<Children>();
        body_commands.with_children(|grid| {
            for (display_index, &row_index) in order.iter().enumerate() {
                let row = &table.rows[row_index];
                for col in 0..table.columns.len() {
                    let value = row.get(col).map_or("", String::as_str);
                    let striped = display_index % 2 == 1;
                    grid.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                            ..default()
                        },
                        BackgroundColor(if striped {
                            theme::INSET_BG.with_alpha(0.55)
                        } else {
                            Color::NONE
                        }),
                    ))
                    .with_children(|cell| {
                        if let Some(builder) = &table.cell_builder {
                            builder(cell, &theme, row_index, col, value);
                        } else {
                            cell.spawn((
                                Text::new(value),
                                theme.font(12.5),
                                TextColor(theme::TEXT),
                                Pickable::IGNORE,
                            ));
                        }
                    });
                }
            }
        });
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (handle_header_clicks, rebuild_table_bodies).chain());
}
