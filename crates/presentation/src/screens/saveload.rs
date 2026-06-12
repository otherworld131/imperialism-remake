//! Native save/load UI: the Save modal (name input, overwrite confirm) and
//! the Load modal (save browser over `./saves/`), plus the Restart confirm.
//!
//! Saves are CLI-compatible `SaveFile` v4 envelopes written through
//! `frontend_api::Session::save` (gzip JSON, `.json.gz`); the browser lists
//! whatever `infrastructure::persistence::list_saves` recognizes (`.json`,
//! `.bin`, `.gz`, `.zst`), so CLI saves load here and vice versa.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::game::resources::SessionRes;
use crate::setup::jobs::{self, ActiveSetupJob};
use crate::setup::{ActiveGameConfig, SetupConfig};
use crate::state::TurnPhase;
use crate::theme::{self, Theme};
use crate::widgets::{
    self, ButtonActivated, ButtonProps, ModalProps, ModalStack, TextInputProps, Toast, UiTextInput,
    open_modal, spawn_button, spawn_text_input,
};

/// Directory native saves live in (relative to the working dir, matching
/// the CLI's `src/saves.rs`).
pub fn saves_dir() -> PathBuf {
    PathBuf::from("saves")
}

#[derive(Component)]
pub struct SaveNameInput;

/// "Save" button in the save modal; carries the modal root for teardown.
#[derive(Component)]
pub struct SaveConfirmBtn {
    pub modal: Entity,
}

/// "Overwrite" button in the overwrite-confirm modal.
#[derive(Component)]
pub struct OverwriteConfirmBtn {
    pub file_name: String,
    /// Both modal roots to tear down on success.
    pub modals: [Entity; 2],
}

/// Per-row "Load" button in the load modal.
#[derive(Component)]
pub struct LoadSaveBtn {
    pub path: PathBuf,
    pub modal: Entity,
}

/// "Restart" button in the restart-confirm modal.
#[derive(Component)]
pub struct RestartConfirmBtn {
    pub modal: Entity,
}

/// Normalize a user-typed save name into a safe file name with a
/// CLI-compatible extension (defaults to gzip JSON).
pub fn normalize_save_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace("..", "_");
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        return None;
    }
    let lower = cleaned.to_lowercase();
    let has_ext = [".json", ".json.gz", ".bin", ".bin.zst", ".gz", ".zst"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    Some(if has_ext {
        cleaned
    } else {
        format!("{cleaned}.json.gz")
    })
}

/// Open the Save modal with a default name of `{mapkey}-turn{N}`.
pub fn open_save_modal(
    commands: &mut Commands,
    stack: &mut ModalStack,
    theme: &Theme,
    default_name: &str,
) {
    let handles = open_modal(
        commands,
        stack,
        theme,
        ModalProps {
            title: "Save Game".into(),
            width: Val::Px(420.0),
        },
    );
    let modal = handles.root;
    commands.entity(handles.content).with_children(|content| {
        content.spawn((
            Text::new("Save name"),
            theme.font(12.0),
            TextColor(theme::TEXT_DIM),
        ));
        let input = spawn_text_input(
            content,
            theme,
            TextInputProps {
                width: Val::Percent(100.0),
                max_len: 48,
                value: default_name.to_string(),
            },
        );
        content.commands().entity(input).insert(SaveNameInput);
        content.spawn((
            Text::new("Written to ./saves/ as a CLI-compatible .json.gz save."),
            theme.font(11.0),
            TextColor(theme::TEXT_DIM),
        ));
        content
            .spawn((Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                column_gap: Val::Px(10.0),
                ..default()
            },))
            .with_children(|row| {
                let save = spawn_button(
                    row,
                    theme,
                    ButtonProps {
                        label: "Save".into(),
                        width: Some(Val::Px(120.0)),
                        ..default()
                    },
                );
                row.commands().entity(save).insert(SaveConfirmBtn { modal });
            });
    });
}

/// Open the Load modal listing every save in `./saves/`.
pub fn open_load_modal(commands: &mut Commands, stack: &mut ModalStack, theme: &Theme) {
    let saves = frontend_api::session::list_saves(&saves_dir());
    let handles = open_modal(
        commands,
        stack,
        theme,
        ModalProps {
            title: "Load Game".into(),
            width: Val::Px(560.0),
        },
    );
    let modal = handles.root;
    commands.entity(handles.content).with_children(|content| {
        if saves.is_empty() {
            content.spawn((
                Text::new("No saves found in ./saves/ yet."),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
            ));
            return;
        }
        let scroll = widgets::spawn_scroll_area(
            content,
            theme,
            widgets::ScrollProps {
                width: Val::Percent(100.0),
                height: Val::Px(360.0),
                ..default()
            },
        );
        content
            .commands()
            .entity(scroll.content)
            .with_children(|list| {
                for save in &saves {
                    list.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(10.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            margin: UiRect::bottom(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(theme::INSET_BG),
                        BorderColor::all(theme::BORDER),
                    ))
                    .with_children(|row| {
                        row.spawn((Node {
                            flex_direction: FlexDirection::Column,
                            flex_grow: 1.0,
                            ..default()
                        },))
                            .with_children(|info| {
                                info.spawn((
                                    Text::new(save.file_name.clone()),
                                    theme.font_bold(12.5),
                                    TextColor(theme::TEXT),
                                ));
                                let meta = if save.turn_display.is_empty() {
                                    "Unreadable metadata".to_string()
                                } else {
                                    format!(
                                        "{} · {} · {} · {}",
                                        save.nation_name,
                                        save.turn_display,
                                        save.difficulty,
                                        save.timestamp
                                    )
                                };
                                info.spawn((
                                    Text::new(meta),
                                    theme.font(10.5),
                                    TextColor(theme::TEXT_DIM),
                                ));
                            });
                        let load = spawn_button(
                            row,
                            theme,
                            ButtonProps {
                                label: "Load".into(),
                                width: Some(Val::Px(80.0)),
                                ..default()
                            },
                        );
                        row.commands().entity(load).insert(LoadSaveBtn {
                            path: save.path.clone(),
                            modal,
                        });
                    });
                }
            });
    });
}

/// Open the Restart confirm modal (web `confirm('Restart this map…')`).
pub fn open_restart_modal(commands: &mut Commands, stack: &mut ModalStack, theme: &Theme) {
    let handles = open_modal(
        commands,
        stack,
        theme,
        ModalProps {
            title: "Restart".into(),
            width: Val::Px(380.0),
        },
    );
    let modal = handles.root;
    commands.entity(handles.content).with_children(|content| {
        content.spawn((
            Text::new("Restart this map from turn 1? All progress is lost."),
            theme.font(12.5),
            TextColor(theme::TEXT),
        ));
        content
            .spawn((Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                column_gap: Val::Px(10.0),
                ..default()
            },))
            .with_children(|row| {
                let restart = spawn_button(
                    row,
                    theme,
                    ButtonProps {
                        label: "Restart".into(),
                        width: Some(Val::Px(120.0)),
                        ..default()
                    },
                );
                row.commands()
                    .entity(restart)
                    .insert(RestartConfirmBtn { modal });
            });
    });
}

/// Save / overwrite / load / restart button plumbing.
#[allow(clippy::too_many_arguments)]
pub fn handle_saveload_buttons(
    mut activations: MessageReader<ButtonActivated>,
    mut commands: Commands,
    theme: Res<Theme>,
    mut stack: ResMut<ModalStack>,
    save_buttons: Query<&SaveConfirmBtn>,
    overwrite_buttons: Query<&OverwriteConfirmBtn>,
    load_buttons: Query<&LoadSaveBtn>,
    restart_buttons: Query<&RestartConfirmBtn>,
    inputs: Query<&UiTextInput, With<SaveNameInput>>,
    session: Res<SessionRes>,
    active_config: Res<ActiveGameConfig>,
    mut active_job: ResMut<ActiveSetupJob>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut toasts: MessageWriter<Toast>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(button) = save_buttons.get(*entity) {
            let Some(raw) = inputs.iter().next().map(|i| i.value.clone()) else {
                continue;
            };
            let Some(file_name) = normalize_save_name(&raw) else {
                toasts.write(Toast::error("Save name cannot be empty."));
                continue;
            };
            if saves_dir().join(&file_name).exists() {
                open_overwrite_modal(&mut commands, &mut stack, &theme, file_name, button.modal);
                continue;
            }
            if write_save(&session, &file_name, &mut toasts) {
                commands.entity(button.modal).despawn();
            }
        } else if let Ok(button) = overwrite_buttons.get(*entity) {
            if write_save(&session, &button.file_name, &mut toasts) {
                for modal in button.modals {
                    commands.entity(modal).despawn();
                }
            }
        } else if let Ok(button) = load_buttons.get(*entity) {
            commands.entity(button.modal).despawn();
            jobs::start_load(&mut active_job, &mut next_phase, button.path.clone());
        } else if let Ok(button) = restart_buttons.get(*entity) {
            commands.entity(button.modal).despawn();
            let Some(config) = restart_config(&active_config, &session) else {
                toasts.write(Toast::error("No start parameters known for this game."));
                continue;
            };
            jobs::start_restart(&mut active_job, &mut next_phase, &config);
        }
    }
}

/// Restart with the nation currently being viewed (web parity: the player
/// may have switched viewpoint in observer mode).
fn restart_config(active: &ActiveGameConfig, session: &SessionRes) -> Option<SetupConfig> {
    let mut config = active.0.clone()?;
    if let Some(session) = session.0.as_ref() {
        let human = session.human_nation();
        let idx = frontend_api::flavor::get_nation_flags(session.game())
            .as_array()
            .and_then(|nations| {
                nations
                    .iter()
                    .filter(|n| n["nation_type"] == "GreatPower")
                    .position(|n| n["nation_id"].as_u64() == Some(u64::from(human)))
            });
        if let Some(idx) = idx {
            config.picked_nation = Some(idx);
        }
    }
    Some(config)
}

fn open_overwrite_modal(
    commands: &mut Commands,
    stack: &mut ModalStack,
    theme: &Theme,
    file_name: String,
    save_modal: Entity,
) {
    let handles = open_modal(
        commands,
        stack,
        theme,
        ModalProps {
            title: "Overwrite save?".into(),
            width: Val::Px(380.0),
        },
    );
    let modal = handles.root;
    commands.entity(handles.content).with_children(|content| {
        content.spawn((
            Text::new(format!("\"{file_name}\" already exists. Overwrite it?")),
            theme.font(12.5),
            TextColor(theme::TEXT),
        ));
        content
            .spawn((Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::FlexEnd,
                column_gap: Val::Px(10.0),
                ..default()
            },))
            .with_children(|row| {
                let overwrite = spawn_button(
                    row,
                    theme,
                    ButtonProps {
                        label: "Overwrite".into(),
                        width: Some(Val::Px(120.0)),
                        ..default()
                    },
                );
                row.commands()
                    .entity(overwrite)
                    .insert(OverwriteConfirmBtn {
                        file_name,
                        modals: [modal, save_modal],
                    });
            });
    });
}

/// Write the live session to `./saves/<file_name>`; returns success.
pub fn write_save(
    session: &SessionRes,
    file_name: &str,
    toasts: &mut MessageWriter<Toast>,
) -> bool {
    let Some(session) = session.0.as_ref() else {
        toasts.write(Toast::error("No game to save."));
        return false;
    };
    let dir = saves_dir();
    if let Err(err) = std::fs::create_dir_all(&dir) {
        toasts.write(Toast::error(format!("Saves directory error: {err}")));
        return false;
    }
    match session.save(&dir.join(file_name)) {
        Ok(()) => {
            toasts.write(Toast::success(format!("Saved \"{file_name}\".")));
            true
        }
        Err(err) => {
            toasts.write(Toast::error(format!("Save failed: {}", err.message())));
            false
        }
    }
}

/// Default save name: `{mapkey}-turn{N}`.
pub fn default_save_name(session: &SessionRes) -> String {
    session
        .0
        .as_ref()
        .map(|s| format!("{}-turn{}", s.map_key(), s.turn_number()))
        .unwrap_or_else(|| "save".to_string())
}
