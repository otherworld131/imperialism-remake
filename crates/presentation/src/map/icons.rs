//! Map icon assets: the 64×64 PNGs under `crates/presentation/assets/icons/`
//! (see `assets-src/icons/MANIFEST.md`), preloaded once at startup.

use bevy::prelude::*;
use std::collections::HashMap;

/// `(group, name)` → image handle, e.g. `("commodities", "Coal")`.
#[derive(Resource, Default)]
pub struct IconAssets {
    icons: HashMap<(String, String), Handle<Image>>,
}

impl IconAssets {
    pub fn get(&self, group: &str, name: &str) -> Option<Handle<Image>> {
        self.icons
            .get(&(group.to_string(), name.to_string()))
            .cloned()
    }
}

/// The asset-root directory the app should serve from: the crate's own
/// `assets/` in dev builds, the working directory's `assets/` when packaged.
pub fn asset_root() -> std::path::PathBuf {
    let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    if dev.is_dir() {
        dev
    } else {
        std::path::PathBuf::from("assets")
    }
}

/// Discover and load every icon under `<asset root>/icons/<group>/<name>.png`.
/// Discovery walks the filesystem; loading goes through the asset server
/// with paths relative to the asset root configured in `app.rs`.
pub fn load_icons(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut icons = HashMap::new();
    let root = asset_root().join("icons");
    if let Ok(groups) = std::fs::read_dir(&root) {
        for group in groups.flatten() {
            let group_name = group.file_name().to_string_lossy().to_string();
            let Ok(files) = std::fs::read_dir(group.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("png") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let relative = format!("icons/{group_name}/{stem}.png");
                icons.insert(
                    (group_name.clone(), stem.to_string()),
                    asset_server.load(relative),
                );
            }
        }
    }
    if icons.is_empty() {
        warn!(
            "no map icons found under {} — markers will be missing",
            root.display()
        );
    }
    commands.insert_resource(IconAssets { icons });
}
