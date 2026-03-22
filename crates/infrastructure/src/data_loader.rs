//! File-system loader for game data definitions.
//!
//! Reads RON files from disk and passes their contents to the domain's
//! `GameData::from_ron_strings()`. The domain never touches the filesystem
//! directly — it receives `&str` content only.

use domain::data::GameData;
use std::path::Path;

/// Load game data from RON files in the given data directory.
///
/// Each section falls back to hardcoded defaults if the corresponding
/// file is missing or fails to parse.
pub fn load_game_data(data_dir: &Path) -> GameData {
    let tech = try_read(data_dir, "definitions/technologies.ron");
    let units = try_read(data_dir, "definitions/units.ron");
    let ships = try_read(data_dir, "definitions/ships.ron");

    GameData::from_ron_strings(tech.as_deref(), units.as_deref(), ships.as_deref())
}

/// Try to read a file relative to the data directory, returning `None` on failure.
fn try_read(base: &Path, relative: &str) -> Option<String> {
    let path = base.join(relative);
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_game_data_with_nonexistent_dir_returns_defaults() {
        let data = load_game_data(Path::new("/nonexistent/path"));
        // Falls back to hardcoded defaults
        assert_eq!(data.tech_tree.all_techs().len(), 28);
        assert_eq!(data.unit_stats.len(), 22);
        assert_eq!(data.ship_stats.len(), 13);
    }

    #[test]
    fn load_game_data_from_data_dir() {
        // Use the actual project data directory
        let data = load_game_data(Path::new("../../data"));
        assert_eq!(data.tech_tree.all_techs().len(), 28);
    }
}
