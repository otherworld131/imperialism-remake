use bevy::prelude::*;
use domain::game_state::GameState;
use domain::hex::HexCoord;
use domain::types::*;

use crate::colors;

/// Size of hex tiles in pixels (outer radius).
pub const HEX_SIZE: f32 = 20.0;

/// Marker for hex tile sprites.
#[derive(Component)]
pub struct HexTileSprite {
    pub coord: HexCoord,
}

/// Marker for hex tile labels.
#[derive(Component)]
pub struct HexTileLabel {
    pub coord: HexCoord,
}

/// Marker for capital star markers.
#[derive(Component)]
pub struct CapitalMarker;

/// Convert hex axial coordinates to pixel position (pointy-top).
pub fn hex_to_pixel(q: i32, r: i32) -> Vec2 {
    let x = HEX_SIZE * (3.0_f32.sqrt() * q as f32 + 3.0_f32.sqrt() / 2.0 * r as f32);
    let y = HEX_SIZE * (3.0 / 2.0 * r as f32);
    Vec2::new(x, -y) // flip Y for screen coordinates
}

/// Render the entire hex map as colored sprites.
pub fn render_hex_map(
    mut commands: Commands,
    game: Res<GameStateResource>,
) {
    let game = &game.0;

    // Build province→nation color lookup
    let mut province_colors: std::collections::HashMap<ProvinceId, bevy::color::Color> =
        std::collections::HashMap::new();
    for nation in &game.nations {
        let color = colors::nation_color(nation.color);
        for pid in &nation.province_ids {
            province_colors.insert(*pid, color);
        }
    }

    // Render each tile
    for (coord, tile) in game.hex_map.all_tiles() {
        let pos = hex_to_pixel(coord.q, coord.r);
        let terrain = tile.terrain();

        // Determine color: nation color for owned land, terrain color for unowned/sea
        let base_color = if terrain == TerrainType::Sea {
            colors::terrain_color(TerrainType::Sea)
        } else if let Some(pid) = tile.province_id {
            province_colors
                .get(&pid)
                .copied()
                .unwrap_or_else(|| colors::terrain_color(terrain))
        } else {
            colors::terrain_color(terrain)
        };

        // Spawn hex tile as a colored sprite
        commands.spawn((
            Sprite {
                color: base_color,
                custom_size: Some(Vec2::new(HEX_SIZE * 1.7, HEX_SIZE * 1.7)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, 0.0),
            HexTileSprite { coord },
        ));

        // Add terrain label for land tiles
        if terrain != TerrainType::Sea {
            let label = colors::terrain_label(terrain);
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                Transform::from_xyz(pos.x, pos.y, 1.0),
                HexTileLabel { coord },
            ));
        }

        // Add star for capital tiles
        if tile.is_capital && terrain != TerrainType::Sea {
            commands.spawn((
                Text2d::new("★"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_xyz(pos.x, pos.y + 2.0, 2.0),
                CapitalMarker,
            ));
        }
    }
}

/// Bevy resource wrapping the game state.
#[derive(Resource)]
pub struct GameStateResource(pub GameState);
