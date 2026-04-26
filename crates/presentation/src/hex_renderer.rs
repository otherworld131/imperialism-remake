use application::domain::game_state::GameState;
use application::domain::hex::HexCoord;
use application::domain::types::*;
use bevy::prelude::*;

use crate::colors;

/// Size of hex tiles in pixels (outer radius).
pub const HEX_SIZE: f32 = 24.0;
/// Visual size of each tile quad.
const TILE_SIZE: f32 = HEX_SIZE * 1.65;

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

/// Resource holding the white pixel texture for colored tiles.
#[derive(Resource)]
pub struct WhitePixel(pub Handle<Image>);

/// Convert hex axial coordinates to pixel position (pointy-top).
pub fn hex_to_pixel(q: i32, r: i32) -> Vec2 {
    let x = HEX_SIZE * (3.0_f32.sqrt() * q as f32 + 3.0_f32.sqrt() / 2.0 * r as f32);
    let y = HEX_SIZE * (3.0 / 2.0 * r as f32);
    Vec2::new(x, -y) // flip Y for screen coordinates
}

/// Create a 2x2 white pixel image AND render the hex map in one system.
/// This avoids startup ordering issues.
pub fn render_hex_map(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    game: Res<GameStateResource>,
) {
    // === Step 1: Create white pixel texture ===
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    let white_img = Image::new_fill(
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    let white_handle = images.add(white_img);
    commands.insert_resource(WhitePixel(white_handle.clone()));

    let game = &game.0;

    // === Step 2: Spawn a test sprite at origin to verify rendering works ===
    println!("=== RENDERING HEX MAP ===");
    println!(
        "Map size: {}x{}, tiles: {}",
        game.hex_map.width(),
        game.hex_map.height(),
        game.hex_map.tile_count()
    );

    // Spawn test squares at MULTIPLE locations to verify rendering
    let map_center = hex_to_pixel(game.hex_map.width() / 2, game.hex_map.height() / 2);

    // Red square at map center (where camera is)
    commands.spawn((
        Sprite {
            image: white_handle.clone(),
            color: Color::srgb(1.0, 0.0, 0.0),
            custom_size: Some(Vec2::new(200.0, 200.0)),
            ..default()
        },
        Transform::from_xyz(map_center.x, map_center.y, 10.0),
    ));
    println!(
        "Spawned RED test square at map center ({}, {})",
        map_center.x, map_center.y
    );

    // Green square at origin
    commands.spawn((
        Sprite {
            image: white_handle.clone(),
            color: Color::srgb(0.0, 1.0, 0.0),
            custom_size: Some(Vec2::new(200.0, 200.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 10.0),
    ));
    println!("Spawned GREEN test square at origin (0, 0)");

    // Blue square at first tile position
    let first_tile_pos = hex_to_pixel(0, 0);
    commands.spawn((
        Sprite {
            image: white_handle.clone(),
            color: Color::srgb(0.0, 0.0, 1.0),
            custom_size: Some(Vec2::new(200.0, 200.0)),
            ..default()
        },
        Transform::from_xyz(first_tile_pos.x, first_tile_pos.y, 10.0),
    ));
    println!(
        "Spawned BLUE test square at first tile ({}, {})",
        first_tile_pos.x, first_tile_pos.y
    );

    // === Step 3: Build province→nation color lookup ===
    let mut province_colors: std::collections::HashMap<ProvinceId, Color> =
        std::collections::HashMap::new();
    for nation in &game.nations {
        let color = colors::nation_color(nation.color);
        for pid in &nation.province_ids {
            province_colors.insert(*pid, color);
        }
    }

    // === Step 4: Render each tile ===
    let mut tile_count = 0u32;
    for (coord, tile) in game.hex_map.all_tiles() {
        let pos = hex_to_pixel(coord.q, coord.r);
        let terrain = tile.terrain();

        let tile_color = if terrain == TerrainType::Sea {
            colors::terrain_color(TerrainType::Sea)
        } else if let Some(pid) = tile.province_id {
            province_colors
                .get(&pid)
                .copied()
                .unwrap_or_else(|| colors::terrain_color(terrain))
        } else {
            colors::terrain_color(terrain)
        };

        commands.spawn((
            Sprite {
                image: white_handle.clone(),
                color: tile_color,
                custom_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y, 0.0),
            HexTileSprite { coord },
        ));

        // Terrain label for land tiles
        if terrain != TerrainType::Sea {
            let label = colors::terrain_label(terrain);
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                Transform::from_xyz(pos.x, pos.y, 1.0),
                HexTileLabel { coord },
            ));
        }

        // Capital stars
        if tile.is_capital && terrain != TerrainType::Sea {
            commands.spawn((
                Text2d::new("★"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_xyz(pos.x, pos.y + 3.0, 2.0),
                CapitalMarker,
            ));
        }

        tile_count += 1;
    }

    println!("Spawned {} tile entities", tile_count);

    // Log where the center of the map is
    let center = hex_to_pixel(game.hex_map.width() / 2, game.hex_map.height() / 2);
    println!("Map center pixel: ({}, {})", center.x, center.y);
}

/// Bevy resource wrapping the game state.
#[derive(Resource)]
pub struct GameStateResource(pub GameState);
