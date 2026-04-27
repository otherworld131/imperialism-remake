use std::path::PathBuf;

use ::infrastructure::persistence;
use ::infrastructure::data_loader::load_embedded_game_data;
use ::infrastructure::PersistenceError;
use domain::game_state::GameState;

#[derive(Debug)]
pub(crate) enum SaveError {
    InvalidFilename { reason: String },
    Io { context: &'static str, source: std::io::Error },
    Persistence(PersistenceError),
}

impl SaveError {
    fn invalid(reason: impl Into<String>) -> Self {
        Self::InvalidFilename { reason: reason.into() }
    }

    fn io(context: &'static str, source: std::io::Error) -> Self {
        Self::Io { context, source }
    }
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFilename { reason } => write!(f, "{reason}"),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Persistence(err) => write!(f, "{err}"),
        }
    }
}

impl From<PersistenceError> for SaveError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

pub(crate) fn saves_dir() -> PathBuf {
    PathBuf::from("saves")
}

pub(crate) fn sanitize_save_filename(filename: &str) -> Result<PathBuf, SaveError> {
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err(SaveError::invalid(
            "Invalid filename: path separators and '..' are not allowed.",
        ));
    }
    let dir = saves_dir();
    let path = dir.join(filename);
    if let Ok(meta) = std::fs::symlink_metadata(&path)
        && meta.file_type().is_symlink()
    {
        return Err(SaveError::invalid("Invalid filename: symlinks are not allowed."));
    }
    let canonical_dir = dir
        .canonicalize()
        .map_err(|e| SaveError::io("Saves directory error", e))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| SaveError::io("File not found", e))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(SaveError::invalid(
            "Invalid filename: path escapes saves directory.",
        ));
    }
    Ok(canonical_path)
}

pub(crate) fn safe_save_path(filename: &str) -> Result<PathBuf, SaveError> {
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err(SaveError::invalid("Invalid filename"));
    }
    let dir = saves_dir();
    let path = dir.join(filename);
    if let Ok(meta) = std::fs::symlink_metadata(&path)
        && meta.file_type().is_symlink()
    {
        return Err(SaveError::invalid("Invalid filename: symlinks are not allowed."));
    }
    let canonical_dir = dir
        .canonicalize()
        .map_err(|e| SaveError::io("Saves directory error", e))?;
    if path.exists() {
        let canonical_path = path
            .canonicalize()
            .map_err(|e| SaveError::io("Path error", e))?;
        if !canonical_path.starts_with(&canonical_dir) {
            return Err(SaveError::invalid(
                "Invalid filename: path escapes saves directory.",
            ));
        }
    }
    Ok(path)
}

pub(crate) fn atomic_save_game(game: &GameState, filename: &str) -> Result<(), SaveError> {
    let dir = saves_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| SaveError::io("Failed to create saves directory", e))?;
    let target = safe_save_path(filename)?;
    let tmp_path = dir.join(format!(".{}.tmp", filename));
    if let Ok(meta) = std::fs::symlink_metadata(&tmp_path)
        && meta.file_type().is_symlink()
    {
        std::fs::remove_file(&tmp_path).ok();
    }
    persistence::save_game(game, &tmp_path)?;
    std::fs::rename(&tmp_path, &target)
        .map_err(|e| SaveError::io("Failed to finalize save", e))?;
    Ok(())
}

pub(crate) fn save_current_game(game: &GameState) {
    let dir = saves_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        println!("  Failed to create saves directory: {}", e);
        return;
    }

    // Show existing saves before saving
    print_save_list(&dir);

    let filename = format!("save_{}_Q{}.json", game.turn.year(), game.turn.quarter());
    let path = dir.join(&filename);

    match persistence::save_game(game, &path) {
        Ok(()) => {
            println!("  Game saved to: saves/{}", filename);
        }
        Err(e) => {
            println!("  Failed to save: {}", e);
        }
    }
}

pub(crate) fn quicksave_game(game: &GameState) {
    match atomic_save_game(game, "quicksave.json") {
        Ok(()) => println!("  Quicksave complete."),
        Err(e) => println!("  Quicksave failed: {}", e),
    }
}

pub(crate) fn list_saved_games() {
    let dir = saves_dir();
    if !dir.exists() {
        println!("  No saved games found.");
        println!();
        println!("  Use: load <filename> (e.g., \"load save_1820_Q1.json\")");
        return;
    }

    println!();
    print_save_list(&dir);
    println!();
    println!("  Use: load <filename> (e.g., \"load save_1820_Q1.json\")");
}

pub(crate) fn print_save_list(dir: &std::path::Path) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect(),
        Err(_) => return,
    };

    if entries.is_empty() {
        return;
    }

    // Sort by modification time, most recent first
    entries.sort_by(|a, b| {
        let time_a = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let time_b = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        time_b.cmp(&time_a)
    });

    println!("  SAVED GAMES:");
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let meta_str = if let Some(meta) = persistence::read_save_metadata(&entry.path()) {
            let size_str = entry
                .metadata()
                .map(|m| {
                    let bytes = m.len();
                    if bytes >= 1_048_576 {
                        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
                    } else if bytes >= 1024 {
                        format!("{:.0} KB", bytes as f64 / 1024.0)
                    } else {
                        format!("{} B", bytes)
                    }
                })
                .unwrap_or_default();
            let ts_display = if meta.timestamp.len() >= 16 {
                // Show "YYYY-MM-DD HH:MM" from ISO 8601
                meta.timestamp[..16].replace('T', " ")
            } else {
                meta.timestamp.clone()
            };
            format!(
                " ({}, {} {}, {})",
                size_str, meta.nation_name, meta.turn_display, ts_display
            )
        } else {
            let size_str = entry
                .metadata()
                .map(|m| {
                    let bytes = m.len();
                    if bytes >= 1_048_576 {
                        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
                    } else if bytes >= 1024 {
                        format!("{:.0} KB", bytes as f64 / 1024.0)
                    } else {
                        format!("{} B", bytes)
                    }
                })
                .unwrap_or_default();
            format!(" ({})", size_str)
        };
        println!("    {}. {}{}", i + 1, name_str, meta_str);
    }
}

pub(crate) fn load_saved_game(filename: &str) -> Result<GameState, SaveError> {
    let path = sanitize_save_filename(filename)?;
    persistence::load_game_with_data(&path, load_embedded_game_data()).map_err(Into::into)
}

pub(crate) fn delete_saved_game(filename: &str) {
    let path = match sanitize_save_filename(filename) {
        Ok(p) => p,
        Err(e) => {
            println!("  Cannot delete: {}", e);
            return;
        }
    };
    match persistence::delete_save(&path) {
        Ok(()) => {
            println!("  Save deleted: {}", filename);
        }
        Err(e) => {
            println!("  Failed to delete: {}", e);
        }
    }
}

pub(crate) fn cmd_saveinfo(filename: &str) {
    let path = match sanitize_save_filename(filename) {
        Ok(p) => p,
        Err(e) => {
            println!("  Cannot read save info: {}", e);
            return;
        }
    };
    match persistence::read_save_metadata(&path) {
        Some(meta) => {
            println!();
            println!("  SAVE FILE INFO: {}", filename);
            println!("    Version:    {}", meta.version);
            println!("    Nation:     {}", meta.nation_name);
            println!("    Turn:       {}", meta.turn_display);
            println!("    Difficulty: {}", meta.difficulty);
            println!("    Timestamp:  {}", meta.timestamp);
        }
        None => {
            println!(
                "  Could not read metadata from '{}'. File may be corrupt or in an old format.",
                filename
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_with_dotdot() {
        let result = sanitize_save_filename("../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path separators"));
    }

    #[test]
    fn rejects_forward_slash() {
        let result = sanitize_save_filename("subdir/save.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path separators"));
    }

    #[test]
    fn rejects_backslash() {
        let result = sanitize_save_filename("subdir\\save.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path separators"));
    }

    #[test]
    fn rejects_embedded_dotdot() {
        let result = sanitize_save_filename("save..json");
        assert!(result.is_err());
    }

    #[test]
    fn accepts_valid_filename_when_file_exists() {
        let dir = saves_dir();
        std::fs::create_dir_all(&dir).ok();
        let test_file = dir.join("_test_sanitize.json");
        std::fs::write(&test_file, "{}").ok();

        let result = sanitize_save_filename("_test_sanitize.json");
        assert!(result.is_ok());

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn rejects_nonexistent_file() {
        let dir = saves_dir();
        std::fs::create_dir_all(&dir).ok();

        let result = sanitize_save_filename("nonexistent_file_xyz.json");
        assert!(result.is_err());
    }
}
