//! Palette shared by every screen. Terrain and nation colors match the web
//! frontend (`web/src/components/HexMap.tsx`) so both UIs read the same.

use bevy::prelude::*;

pub const BG: Color = Color::srgb_u8(0x1a, 0x1a, 0x2e);
pub const TEXT: Color = Color::srgb_u8(0xe0, 0xd8, 0xc0);
pub const GOLD: Color = Color::srgb_u8(0xda, 0xa5, 0x20);

pub const PANEL_BG: Color = Color::srgba(0.10, 0.10, 0.18, 0.92);
pub const BUTTON_BG: Color = Color::srgb(0.18, 0.15, 0.08);
pub const BUTTON_BG_HOVER: Color = Color::srgb(0.26, 0.21, 0.10);
pub const BUTTON_BG_PRESSED: Color = Color::srgb(0.36, 0.28, 0.10);
pub const OVERLAY_BG: Color = Color::srgba(0.05, 0.05, 0.10, 0.62);

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
