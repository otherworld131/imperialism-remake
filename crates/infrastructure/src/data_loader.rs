//! File-system loader for game data definitions.
//!
//! Reads RON files from disk and passes their contents to the domain's
//! `GameData::from_ron_strings()`. The domain never touches the filesystem
//! directly — it receives `&str` content only.

use domain::data::GameData;
use std::path::Path;

/// Load game data from RON files in the given data directory.
///
/// `technologies.ron` is required: panics if the file is missing or unreadable.
/// `units.ron` and `ships.ron` fall back to hardcoded defaults if absent.
pub fn load_game_data(data_dir: &Path) -> GameData {
    let tech = read_required(data_dir, "definitions/technologies.ron");
    let units = try_read(data_dir, "definitions/units.ron");
    let ships = try_read(data_dir, "definitions/ships.ron");

    GameData::from_ron_strings(Some(&tech), units.as_deref(), ships.as_deref())
}

/// Read a required file relative to the data directory. Panics if it cannot be read.
fn read_required(base: &Path, relative: &str) -> String {
    let path = base.join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Required data file '{}' could not be read: {}", path.display(), e))
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
    fn load_game_data_from_data_dir() {
        // Use the actual project data directory
        let data = load_game_data(Path::new("../../data"));
        assert_eq!(data.tech_tree.all_techs().len(), 28);
    }
}
