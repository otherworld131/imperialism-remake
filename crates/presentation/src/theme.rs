//! Palette + font handles shared by every screen. Terrain and nation colors
//! match the web frontend (`web/src/components/HexMap.tsx`) so both UIs read
//! the same. The [`Theme`] resource carries font handles and small helpers;
//! colors stay module constants so systems can use them without the resource.

use bevy::prelude::*;

pub const BG: Color = Color::srgb_u8(0x1a, 0x1a, 0x2e);
pub const TEXT: Color = Color::srgb_u8(0xe0, 0xd8, 0xc0);
pub const TEXT_DIM: Color = Color::srgb_u8(0x8f, 0x8a, 0x78);
pub const GOLD: Color = Color::srgb_u8(0xda, 0xa5, 0x20);
pub const BORDER: Color = Color::srgb_u8(0x3a, 0x35, 0x20);

pub const PANEL_BG: Color = Color::srgba(0.10, 0.10, 0.18, 0.92);
pub const PANEL_BG_SOLID: Color = Color::srgb_u8(0x1a, 0x1a, 0x2e);
pub const INSET_BG: Color = Color::srgb_u8(0x12, 0x12, 0x20);
pub const BUTTON_BG: Color = Color::srgb(0.18, 0.15, 0.08);
pub const BUTTON_BG_HOVER: Color = Color::srgb(0.26, 0.21, 0.10);
pub const BUTTON_BG_PRESSED: Color = Color::srgb(0.36, 0.28, 0.10);
pub const BUTTON_BG_DISABLED: Color = Color::srgb(0.13, 0.12, 0.10);
pub const OVERLAY_BG: Color = Color::srgba(0.05, 0.05, 0.10, 0.62);

pub const SUCCESS: Color = Color::srgb_u8(0x6a, 0xb0, 0x4c);
pub const ERROR: Color = Color::srgb_u8(0xc8, 0x4c, 0x3c);

/// Handles to the bundled OFL fonts. Each falls back to Bevy's default font
/// when the TTF is missing on disk, so the app degrades instead of rendering
/// invisible text.
#[derive(Clone, Default)]
pub struct FontHandles {
    pub regular: Handle<Font>,
    pub semibold: Handle<Font>,
    pub italic: Handle<Font>,
    /// UnifrakturCook Bold — reserved for the newspaper masthead.
    pub blackletter: Handle<Font>,
}

/// Shared look-and-feel resource consumed by every `widgets::spawn_*`
/// constructor.
#[derive(Resource)]
pub struct Theme {
    pub fonts: FontHandles,
}

impl Theme {
    pub fn font(&self, size: f32) -> TextFont {
        TextFont {
            font: self.fonts.regular.clone(),
            font_size: size,
            ..default()
        }
    }

    pub fn font_bold(&self, size: f32) -> TextFont {
        TextFont {
            font: self.fonts.semibold.clone(),
            font_size: size,
            ..default()
        }
    }

    pub fn font_italic(&self, size: f32) -> TextFont {
        TextFont {
            font: self.fonts.italic.clone(),
            font_size: size,
            ..default()
        }
    }

    pub fn font_blackletter(&self, size: f32) -> TextFont {
        TextFont {
            font: self.fonts.blackletter.clone(),
            font_size: size,
            ..default()
        }
    }
}

impl FromWorld for Theme {
    fn from_world(world: &mut World) -> Self {
        let Some(mut fonts) = world.get_resource_mut::<Assets<Font>>() else {
            // Headless test worlds have no font assets; default handles are
            // fine because nothing renders there.
            return Theme {
                fonts: FontHandles::default(),
            };
        };
        Theme {
            fonts: FontHandles {
                regular: load_font(&mut fonts, "SourceSerif4-Regular.ttf"),
                semibold: load_font(&mut fonts, "SourceSerif4-Semibold.ttf"),
                italic: load_font(&mut fonts, "SourceSerif4-It.ttf"),
                blackletter: load_font(&mut fonts, "UnifrakturCook-Bold.ttf"),
            },
        }
    }
}

/// Read a bundled TTF from disk and register it as a font asset.
fn load_font(fonts: &mut Assets<Font>, file: &str) -> Handle<Font> {
    let candidates = [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/fonts")
            .join(file),
        crate::map::icons::asset_root().join("fonts").join(file),
        std::path::PathBuf::from("assets/fonts").join(file),
    ];
    for path in &candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        match Font::try_from_bytes(bytes) {
            Ok(font) => return fonts.add(font),
            Err(err) => warn!("failed to parse font {}: {err}", path.display()),
        }
    }
    warn!("font {file} not found; falling back to the default font");
    Handle::default()
}

pub fn terrain_color(terrain: &str) -> Color {
    match terrain {
        "Grassland" => Color::srgb_u8(0xa8, 0xb8, 0x60),
        "Hills" => Color::srgb_u8(0x9a, 0x8a, 0x68),
        "Forest" => Color::srgb_u8(0x3a, 0x7a, 0x3a),
        "Mountain" => Color::srgb_u8(0x7a, 0x70, 0x68),
        "Desert" => Color::srgb_u8(0xd8, 0xc8, 0x88),
        "Swamp" => Color::srgb_u8(0x5a, 0x7a, 0x5a),
        "Tundra" => Color::srgb_u8(0xb8, 0xc8, 0xd0),
        "Sea" => Color::srgb_u8(0x4a, 0x88, 0xb8),
        _ => Color::srgb(0.5, 0.5, 0.5),
    }
}

/// Nation color by the `owner_color` name the map API emits.
pub fn nation_color(name: &str) -> Color {
    match name {
        "Yellow" => Color::srgb_u8(0xff, 0xd9, 0x00),
        "Orange" => Color::srgb_u8(0xff, 0x8c, 0x00),
        "LightBlue" => Color::srgb_u8(0x66, 0xb3, 0xff),
        "Red" => Color::srgb_u8(0xe6, 0x26, 0x26),
        "Green" => Color::srgb_u8(0x1a, 0xbf, 0x1a),
        "Purple" => Color::srgb_u8(0xa6, 0x33, 0xd9),
        "Blue" => Color::srgb_u8(0x33, 0x59, 0xe6),
        "Crimson" => Color::srgb_u8(0xb0, 0x00, 0x20),
        "Magenta" => Color::srgb_u8(0xd9, 0x13, 0xa8),
        "Forest" => Color::srgb_u8(0x1f, 0x5b, 0x2c),
        "Gold" => Color::srgb_u8(0xd4, 0xa5, 0x2a),
        "Aqua" => Color::srgb_u8(0x00, 0xb8, 0xc4),
        "Violet" => Color::srgb_u8(0x8a, 0x2b, 0xe2),
        "BurntOrange" => Color::srgb_u8(0xcc, 0x55, 0x00),
        "HotPink" => Color::srgb_u8(0xff, 0x44, 0xa0),
        "Turquoise" => Color::srgb_u8(0x14, 0xb8, 0x9c),
        "Slate" => Color::srgb_u8(0x5a, 0x6e, 0x8c),
        "Mauve" => Color::srgb_u8(0xb0, 0x7a, 0xb0),
        "Sage" => Color::srgb_u8(0x7a, 0x9b, 0x6a),
        "Mustard" => Color::srgb_u8(0xb8, 0x8a, 0x00),
        "Gray" => Color::srgb_u8(0x99, 0x99, 0x99),
        "Brown" => Color::srgb_u8(0x8c, 0x59, 0x26),
        "Pink" => Color::srgb_u8(0xff, 0x80, 0xb3),
        "Teal" => Color::srgb_u8(0x00, 0xb3, 0xa6),
        "Olive" => Color::srgb_u8(0x80, 0x80, 0x00),
        "Maroon" => Color::srgb_u8(0x8c, 0x00, 0x1a),
        "Navy" => Color::srgb_u8(0x00, 0x00, 0x8c),
        "Cyan" => Color::srgb_u8(0x00, 0xcc, 0xcc),
        "Lime" => Color::srgb_u8(0x73, 0xd9, 0x00),
        "Coral" => Color::srgb_u8(0xff, 0x80, 0x59),
        "Lavender" => Color::srgb_u8(0xb3, 0x80, 0xe6),
        "Tan" => Color::srgb_u8(0xcc, 0xb3, 0x80),
        "Salmon" => Color::srgb_u8(0xff, 0x8c, 0x73),
        "Khaki" => Color::srgb_u8(0xbf, 0xb3, 0x66),
        "Indigo" => Color::srgb_u8(0x4d, 0x00, 0x80),
        "Beige" => Color::srgb_u8(0xe8, 0xd8, 0xb0),
        _ => Color::srgb(0.53, 0.53, 0.53),
    }
}

/// Political-map fill: nation color blended 45% toward white, matching the
/// web frontend's `politicalFill`.
pub fn political_tint(color: Color) -> Color {
    let c = color.to_srgba();
    Color::srgb(
        c.red + (1.0 - c.red) * 0.45,
        c.green + (1.0 - c.green) * 0.45,
        c.blue + (1.0 - c.blue) * 0.45,
    )
}

/// Incorporated-minor fill: nation color blended 65% toward white, matching
/// the web frontend's `incorporatedFill`.
pub fn incorporated_tint(color: Color) -> Color {
    let c = color.to_srgba();
    Color::srgb(
        c.red + (1.0 - c.red) * 0.65,
        c.green + (1.0 - c.green) * 0.65,
        c.blue + (1.0 - c.blue) * 0.65,
    )
}

/// Terrain-map fill: terrain color blended toward the nation color by
/// `amount` (0.15 normal, 0.10 incorporated minors), matching `tintColor`.
pub fn terrain_nation_tint(terrain: Color, nation: Color, amount: f32) -> Color {
    let t = terrain.to_srgba();
    let n = nation.to_srgba();
    Color::srgb(
        t.red * (1.0 - amount) + n.red * amount,
        t.green * (1.0 - amount) + n.green * amount,
        t.blue * (1.0 - amount) + n.blue * amount,
    )
}

/// Diplomatic-overlay status color (`DIPLO_STATUS_COLORS` in HexMap.tsx).
pub fn diplo_status_color(status: &str) -> Color {
    match status {
        "Alliance" => Color::srgb_u8(0x2e, 0xcc, 0x40),
        "NAP" => Color::srgb_u8(0x7f, 0xdb, 0xff),
        "At War" => Color::srgb_u8(0xff, 0x41, 0x36),
        "Neutral" => Color::srgb_u8(0xaa, 0xaa, 0xaa),
        _ => Color::srgb_u8(0x66, 0x66, 0x66),
    }
}

/// The gold used for the perspective nation in overlay modes.
pub const OVERLAY_SELF: Color = Color::srgb_u8(0xff, 0xd9, 0x00);

/// Relationship score (-100..+100) → red → gray → green, matching
/// `scoreToColor` in HexMap.tsx.
pub fn score_color(score: f32) -> Color {
    let t = ((score + 100.0) / 200.0).clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.5 {
        let s = t / 0.5;
        (
            220.0 + (160.0 - 220.0) * s,
            40.0 + (160.0 - 40.0) * s,
            40.0 + (160.0 - 40.0) * s,
        )
    } else {
        let s = (t - 0.5) / 0.5;
        (
            160.0 + (40.0 - 160.0) * s,
            160.0 + (200.0 - 160.0) * s,
            160.0 + (40.0 - 160.0) * s,
        )
    };
    Color::srgb(r / 255.0, g / 255.0, b / 255.0)
}

/// Strength score (-100..+100) → red → yellow → green, matching
/// `strengthToColor` in HexMap.tsx.
pub fn strength_color(score: f32) -> Color {
    let t = ((score + 100.0) / 200.0).clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.5 {
        let s = t / 0.5;
        (220.0 + (200.0 - 220.0) * s, 40.0 + (200.0 - 40.0) * s, 40.0)
    } else {
        let s = (t - 0.5) / 0.5;
        (200.0 + (40.0 - 200.0) * s, 200.0, 40.0)
    };
    Color::srgb(r / 255.0, g / 255.0, b / 255.0)
}
