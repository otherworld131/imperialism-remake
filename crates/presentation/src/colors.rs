use application::{NationColor, TerrainType};
use bevy::color::Color;

/// Map NationColor to Bevy RGBA color.
pub fn nation_color(color: NationColor) -> Color {
    match color {
        NationColor::Yellow => Color::srgb(1.0, 0.85, 0.0),
        NationColor::Orange => Color::srgb(1.0, 0.55, 0.0),
        NationColor::LightBlue => Color::srgb(0.4, 0.7, 1.0),
        NationColor::Red => Color::srgb(0.9, 0.15, 0.15),
        NationColor::Green => Color::srgb(0.1, 0.75, 0.1),
        NationColor::Purple => Color::srgb(0.65, 0.2, 0.85),
        NationColor::Blue => Color::srgb(0.2, 0.35, 0.9),
        NationColor::Crimson => Color::srgb(0.69, 0.0, 0.13),
        NationColor::Magenta => Color::srgb(0.85, 0.07, 0.66),
        NationColor::Forest => Color::srgb(0.12, 0.36, 0.17),
        NationColor::Gold => Color::srgb(0.83, 0.65, 0.16),
        NationColor::Aqua => Color::srgb(0.0, 0.72, 0.77),
        NationColor::Violet => Color::srgb(0.54, 0.17, 0.89),
        NationColor::BurntOrange => Color::srgb(0.8, 0.33, 0.0),
        NationColor::HotPink => Color::srgb(1.0, 0.27, 0.63),
        NationColor::Turquoise => Color::srgb(0.08, 0.72, 0.61),
        NationColor::Slate => Color::srgb(0.35, 0.43, 0.55),
        NationColor::Mauve => Color::srgb(0.69, 0.48, 0.69),
        NationColor::Sage => Color::srgb(0.48, 0.61, 0.42),
        NationColor::Mustard => Color::srgb(0.72, 0.54, 0.0),
        NationColor::Gray => Color::srgb(0.6, 0.6, 0.6),
        NationColor::Brown => Color::srgb(0.55, 0.35, 0.15),
        NationColor::Pink => Color::srgb(1.0, 0.5, 0.7),
        NationColor::Teal => Color::srgb(0.0, 0.7, 0.65),
        NationColor::Olive => Color::srgb(0.5, 0.5, 0.0),
        NationColor::Maroon => Color::srgb(0.55, 0.0, 0.1),
        NationColor::Navy => Color::srgb(0.0, 0.0, 0.55),
        NationColor::Cyan => Color::srgb(0.0, 0.8, 0.8),
        NationColor::Lime => Color::srgb(0.45, 0.85, 0.0),
        NationColor::Coral => Color::srgb(1.0, 0.5, 0.35),
        NationColor::Lavender => Color::srgb(0.7, 0.5, 0.9),
        NationColor::Tan => Color::srgb(0.8, 0.7, 0.5),
        NationColor::Salmon => Color::srgb(1.0, 0.55, 0.45),
        NationColor::Khaki => Color::srgb(0.75, 0.7, 0.4),
        NationColor::Indigo => Color::srgb(0.3, 0.0, 0.5),
    }
}

/// Map TerrainType to a display color.
pub fn terrain_color(terrain: TerrainType) -> Color {
    match terrain {
        TerrainType::Grassland => Color::srgb(0.7, 0.78, 0.42),
        TerrainType::Hills => Color::srgb(0.55, 0.55, 0.35),
        TerrainType::Forest => Color::srgb(0.15, 0.5, 0.18),
        TerrainType::Mountain => Color::srgb(0.5, 0.45, 0.4),
        TerrainType::Desert => Color::srgb(0.85, 0.8, 0.55),
        TerrainType::Swamp => Color::srgb(0.3, 0.4, 0.3),
        TerrainType::Tundra => Color::srgb(0.75, 0.8, 0.85),
        TerrainType::Sea => Color::srgb(0.15, 0.3, 0.65),
    }
}

/// Terrain type abbreviation for hex labels.
pub fn terrain_label(terrain: TerrainType) -> &'static str {
    match terrain {
        TerrainType::Grassland => "G",
        TerrainType::Hills => "H",
        TerrainType::Forest => "F",
        TerrainType::Mountain => "M",
        TerrainType::Desert => "D",
        TerrainType::Swamp => "S",
        TerrainType::Tundra => "T",
        TerrainType::Sea => "~",
    }
}
