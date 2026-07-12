//! Persistent interface scale: one multiplier applied through Bevy's global
//! [`UiScale`] resource, so every UI panel, text run, and icon grows
//! together (the map itself is world-space and scales with camera zoom).
//! Mirrors the web frontend's side-panel font-size slider.
//!
//! Adjustable from the side panel's "UI" section and via Ctrl+`+` /
//! Ctrl+`-` / Ctrl+`0` on any screen. The value persists to
//! `./settings.json` next to `./saves/`.

use bevy::prelude::*;
use bevy::ui::UiScale;
use serde::{Deserialize, Serialize};

pub const MIN_SCALE: f32 = 0.8;
pub const MAX_SCALE: f32 = 2.5;
/// Fresh-install default: the UI was authored small against a 1280×720
/// window; 175% (the previous ceiling) is the intended "normal" reading
/// size on modern displays, with headroom up to 250%.
pub const DEFAULT_SCALE: f32 = 1.75;
const HOTKEY_STEP: f32 = 0.1;
const SETTINGS_FILE: &str = "settings.json";

/// On-disk shape of `./settings.json`. Unknown fields are round-tripped so
/// a scale change never erases settings written by newer code.
#[derive(Serialize, Deserialize, Default)]
struct PersistedSettings {
    ui_scale: Option<f32>,
    /// Side-panel "Debug" disclosure state (collapsed by default).
    debug_panel_expanded: Option<bool>,
    #[serde(flatten)]
    other: serde_json::Map<String, serde_json::Value>,
}

fn read_settings() -> PersistedSettings {
    std::fs::read_to_string(SETTINGS_FILE)
        .ok()
        .and_then(|raw| serde_json::from_str::<PersistedSettings>(&raw).ok())
        .unwrap_or_default()
}

fn write_settings(settings: &PersistedSettings) {
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(err) = std::fs::write(SETTINGS_FILE, json) {
                warn!("failed to persist {SETTINGS_FILE}: {err}");
            }
        }
        Err(err) => warn!("failed to serialize settings: {err}"),
    }
}

/// Whether the side panel's Debug section starts expanded (default: no).
pub fn load_debug_expanded() -> bool {
    read_settings().debug_panel_expanded.unwrap_or(false)
}

pub fn save_debug_expanded(expanded: bool) {
    let mut settings = read_settings();
    settings.debug_panel_expanded = Some(expanded);
    write_settings(&settings);
}

pub fn load_ui_scale() -> f32 {
    // Debug/screenshot hook: `UI_SCALE=1.75` pins the scale for a run
    // without touching settings.json.
    if let Some(scale) = std::env::var("UI_SCALE")
        .ok()
        .and_then(|raw| raw.parse::<f32>().ok())
    {
        return scale.clamp(MIN_SCALE, MAX_SCALE);
    }
    read_settings()
        .ui_scale
        .map_or(DEFAULT_SCALE, |scale| scale.clamp(MIN_SCALE, MAX_SCALE))
}

fn save_ui_scale(scale: f32) {
    // Read-modify-write so future settings fields survive a scale change.
    let mut settings = read_settings();
    settings.ui_scale = Some(scale);
    write_settings(&settings);
}

/// Set, clamp, and persist a new scale.
pub fn apply_scale(ui_scale: &mut UiScale, scale: f32) {
    let scale = (scale * 100.0).round() / 100.0;
    let scale = scale.clamp(MIN_SCALE, MAX_SCALE);
    if (ui_scale.0 - scale).abs() > f32::EPSILON {
        ui_scale.0 = scale;
        save_ui_scale(scale);
    }
}

/// Ctrl+`+` / Ctrl+`-` step the interface scale, Ctrl+`0` resets it —
/// available on every screen (browser-style zoom keys).
pub fn ui_scale_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<bevy::input_focus::InputFocus>,
    mut ui_scale: ResMut<UiScale>,
) {
    if focus.0.is_some()
        || !(keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
    {
        return;
    }
    let current = ui_scale.0;
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        apply_scale(&mut ui_scale, current + HOTKEY_STEP);
    } else if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        apply_scale(&mut ui_scale, current - HOTKEY_STEP);
    } else if keys.just_pressed(KeyCode::Digit0) || keys.just_pressed(KeyCode::Numpad0) {
        apply_scale(&mut ui_scale, DEFAULT_SCALE);
    }
}
