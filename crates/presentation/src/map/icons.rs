//! Map icon assets: the 64×64 PNGs under `crates/presentation/assets/icons/`
//! (see `assets-src/icons/MANIFEST.md`), preloaded once at startup.

use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// The repeating ground textures the map renderer expects under
/// `icons/ground/` — every terrain type plus the sea. A missing entry is
/// not fatal (tiles fall back to flat fills) but is warned about loudly.
pub const GROUND_TEXTURES: [&str; 8] = [
    "Grassland",
    "Hills",
    "Forest",
    "Mountain",
    "Desert",
    "Swamp",
    "Tundra",
    "Sea",
];

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

    /// Test-only builder: register `(group, name)` keys with default
    /// (unloaded) handles so lookup-driven logic can be exercised.
    #[cfg(test)]
    pub fn for_test(entries: &[(&str, &str)]) -> Self {
        Self {
            icons: entries
                .iter()
                .map(|(group, name)| ((group.to_string(), name.to_string()), Handle::default()))
                .collect(),
        }
    }
}

/// The asset-root directory the app should serve from: the crate's own
/// `assets/` in dev builds, then `assets/` beside the executable or current
/// working directory when packaged.
pub fn asset_root() -> std::path::PathBuf {
    let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    if dev.is_dir() {
        return dev;
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let packaged = dir.join("assets");
        if packaged.is_dir() {
            return packaged;
        }
    }
    std::path::PathBuf::from("assets")
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
                // Icons are pixel art: sample with nearest-neighbor so the
                // pixels stay crisp at any map zoom instead of blurring.
                // Ground textures additionally repeat: the map tiles them
                // across merged meshes with world-space UVs. Rail textures
                // repeat along U only (the track tiles down its length; V
                // clamps across the track width).
                let sampler = if group_name == "ground" || group_name == "rail" {
                    ImageSampler::Descriptor(ImageSamplerDescriptor {
                        address_mode_u: ImageAddressMode::Repeat,
                        address_mode_v: if group_name == "rail" {
                            ImageAddressMode::ClampToEdge
                        } else {
                            ImageAddressMode::Repeat
                        },
                        mag_filter: ImageFilterMode::Nearest,
                        min_filter: ImageFilterMode::Nearest,
                        mipmap_filter: ImageFilterMode::Nearest,
                        ..ImageSamplerDescriptor::default()
                    })
                } else {
                    ImageSampler::nearest()
                };
                icons.insert(
                    (group_name.clone(), stem.to_string()),
                    asset_server.load_with_settings(
                        relative,
                        move |settings: &mut ImageLoaderSettings| {
                            settings.sampler = sampler.clone();
                        },
                    ),
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
    for name in GROUND_TEXTURES {
        if !icons.contains_key(&("ground".to_string(), name.to_string())) {
            warn!("ground texture icons/ground/{name}.png missing — those tiles render flat fills");
        }
    }
    commands.insert_resource(IconAssets { icons });
}
