use bevy::color::Color;
use domain::nation::NationColor;
use domain::types::TerrainType;

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
        TerrainType::Farm => Color::srgb(0.6, 0.8, 0.2),
        TerrainType::HardwoodForest => Color::srgb(0.1, 0.5, 0.15),
        TerrainType::ScrubForest => Color::srgb(0.3, 0.55, 0.25),
        TerrainType::FertileHills => Color::srgb(0.55, 0.7, 0.35),
        TerrainType::BarrenHills => Color::srgb(0.55, 0.45, 0.3),
        TerrainType::Mountain => Color::srgb(0.5, 0.45, 0.4),
        TerrainType::Sea => Color::srgb(0.15, 0.3, 0.65),
        TerrainType::DryPlains => Color::srgb(0.8, 0.75, 0.5),
        TerrainType::Plantation => Color::srgb(0.4, 0.7, 0.3),
        TerrainType::OpenRange => Color::srgb(0.65, 0.75, 0.4),
        TerrainType::HorseRanch => Color::srgb(0.7, 0.65, 0.35),
        TerrainType::Orchard => Color::srgb(0.5, 0.75, 0.25),
        TerrainType::Swamp => Color::srgb(0.3, 0.4, 0.3),
        TerrainType::Desert => Color::srgb(0.85, 0.8, 0.55),
        TerrainType::Tundra => Color::srgb(0.75, 0.8, 0.85),
    }
}

/// Terrain type abbreviation for hex labels.
pub fn terrain_label(terrain: TerrainType) -> &'static str {
    match terrain {
        TerrainType::Farm => "F",
        TerrainType::HardwoodForest => "f",
        TerrainType::ScrubForest => "s",
        TerrainType::FertileHills => "H",
        TerrainType::BarrenHills => "h",
        TerrainType::Mountain => "M",
        TerrainType::Sea => "~",
        TerrainType::DryPlains => ".",
        TerrainType::Plantation => "P",
        TerrainType::OpenRange => "R",
        TerrainType::HorseRanch => "r",
        TerrainType::Orchard => "O",
        TerrainType::Swamp => "S",
        TerrainType::Desert => "D",
        TerrainType::Tundra => "T",
    }
}
