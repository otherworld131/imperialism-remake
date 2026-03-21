//! Platform abstraction traits.
//! These define contracts for platform-specific services.
//! Implementations live in infrastructure/presentation crates.

use std::path::PathBuf;

/// File paths, clipboard, notifications — platform-specific services.
pub trait PlatformServices {
    /// Get the user data directory (for saves, settings).
    fn user_data_dir(&self) -> PathBuf;
    /// Get the application directory.
    fn app_dir(&self) -> PathBuf;
}

/// Thin abstraction over rendering.
pub trait RenderSurface {
    /// Clear the screen.
    fn clear(&mut self);
    /// Draw text at position.
    fn draw_text(&mut self, x: f32, y: f32, text: &str);
    /// Get screen dimensions.
    fn dimensions(&self) -> (u32, u32);
}

/// Audio playback abstraction.
pub trait AudioEngine {
    /// Play background music by name.
    fn play_bgm(&mut self, name: &str);
    /// Play a sound effect.
    fn play_sfx(&mut self, name: &str);
    /// Set master volume (0.0 - 1.0).
    fn set_volume(&mut self, volume: f32);
    /// Stop all audio.
    fn stop_all(&mut self);
}

/// Input abstraction.
pub trait InputProvider {
    /// Check if a key is pressed.
    fn is_key_pressed(&self, key: &str) -> bool;
    /// Get mouse position.
    fn mouse_position(&self) -> (f32, f32);
    /// Check if mouse button is pressed.
    fn is_mouse_pressed(&self) -> bool;
}

/// Script loading abstraction (for future Lua integration).
pub trait ScriptLoader {
    /// Load a script from a path.
    fn load_script(&self, path: &str) -> Result<String, String>;
    /// Check if a script exists.
    fn script_exists(&self, path: &str) -> bool;
}
