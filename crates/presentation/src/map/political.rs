//! CPU-rasterized political-map land layer (card #531).
//!
//! The terrain map is chunky pixel art, but political mode used to render
//! smooth anti-aliased organic borders over flat vector fills — it read as a
//! modern atlas next to the pixel terrain. This module instead rasterizes
//! the political fills, country borders and coastlines into one coarse
//! nearest-sampled texture — the same approach as the newspaper archive's
//! historical-map modal (`screens/minimap.rs`), scaled up to world space —
//! so nation shapes render as chunky pixel staircases in the political
//! family of map modes (Political + the diplomatic/military overlays).
//!
//! Terrain mode keeps the organic vector pipeline untouched; the "Organic
//! borders" display toggle therefore now applies to terrain-mode coasts,
//! borders and rivers only.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;

use crate::game::vm::MapTile;
use crate::map::borders;
use crate::map::geometry::{self, HEX_SIZE, SQRT_3};
use crate::map::layers::{GROUND_TEX_WORLD, MapMode, tile_fill_color};

/// World units per political texel: 4× the terrain ground texel (the 64px
/// ground art repeats every [`GROUND_TEX_WORLD`] units), so political pixels
/// read as the same art family, one register chunkier — a hex is about
/// 18 texels wide, matching the blocky look of the news-archive map modal.
pub const POLITICAL_TEXEL: f32 = GROUND_TEX_WORLD / 64.0 * 4.0;

/// Border ink baked into the raster, matching the organic stroke color
/// (`srgba(10/255, 5/255, 0, 0.9)` in `layers.rs`).
const BORDER_RGB: [u8; 3] = [10, 5, 0];
const BORDER_ALPHA: f32 = 0.9;

/// Texel key for sea / off-map — never a border between two such texels.
const SEA_KEY: u32 = u32::MAX;

/// One built political raster: the image plus the world rect it maps onto.
pub struct PoliticalRaster {
    pub image: Image,
    /// World-space rect covered by the image (UV (0,0) at `(min.x, max.y)`,
    /// i.e. image row 0 is the top of the map).
    pub min: Vec2,
    pub max: Vec2,
}

/// Raster cache; rebuilt only when the map version or mode moves, so
/// display-toggle rebuilds of the layer stack reuse the pixels.
#[derive(Resource, Default)]
pub struct PoliticalRasterCache {
    pub key: Option<(u64, MapMode)>,
    pub handle: Handle<Image>,
    pub min: Vec2,
    pub max: Vec2,
}

/// Rasterize the political land layer for `mode`. Sea and off-map texels are
/// transparent (the pixel-art water underlay shows through), land texels get
/// the mode's fill color, and any texel boundary that crosses a country or
/// coast line is inked dark — one texel on each side, so borders come out
/// two texels (~one ground-art "pixel" pair) thick.
///
/// The image spans exactly one horizontal wrap period, so the three wrap
/// copies tile seamlessly; texel→hex lookup wraps through
/// [`borders::wrap_axial`] like every other map pass.
pub fn build_raster(
    tiles: &[MapTile],
    mode: MapMode,
    fill_map: &HashMap<String, Color>,
) -> Option<PoliticalRaster> {
    if tiles.is_empty() {
        return None;
    }
    let map_width = tiles[0].map_width;

    // World-space bounds of the hex centers.
    let mut min_c = Vec2::splat(f32::INFINITY);
    let mut max_c = Vec2::splat(f32::NEG_INFINITY);
    for tile in tiles {
        let p = geometry::hex_to_world(tile.q, tile.r);
        min_c = min_c.min(p);
        max_c = max_c.max(p);
    }

    // Horizontal: exactly one wrap period, starting at the leftmost hex
    // edge; the per-texel width is nudged so the period divides evenly and
    // wrap copies abut without seams. Vertical: hex extents plus nothing to
    // wrap, at the exact texel size.
    let period = geometry::world_width_px(map_width);
    let x0 = min_c.x - SQRT_3 / 2.0 * HEX_SIZE;
    let y_top = max_c.y + HEX_SIZE;
    let y_bottom = min_c.y - HEX_SIZE;
    let n_x = ((period / POLITICAL_TEXEL).round() as usize).max(1);
    let texel_w = period / n_x as f32;
    let n_y = (((y_top - y_bottom) / POLITICAL_TEXEL).ceil() as usize).max(1);

    // Per-tile border-group key (country borders + coasts) and fill bytes.
    let index: HashMap<(i32, i32), usize> = tiles
        .iter()
        .enumerate()
        .map(|(i, t)| ((t.q, t.r), i))
        .collect();
    let mut group_ids: HashMap<&str, u32> = HashMap::new();
    let mut tile_keys: Vec<u32> = Vec::with_capacity(tiles.len());
    let mut tile_fills: Vec<[u8; 4]> = Vec::with_capacity(tiles.len());
    for tile in tiles {
        if tile.is_sea() {
            tile_keys.push(SEA_KEY);
            tile_fills.push([0, 0, 0, 0]);
        } else {
            let next = group_ids.len() as u32;
            tile_keys.push(
                *group_ids
                    .entry(tile.visual_group_or_owner())
                    .or_insert(next),
            );
            tile_fills.push(rgba(tile_fill_color(tile, mode, fill_map)));
        }
    }

    // Fill pass: texel center → hex (wrapped) → key + fill.
    let mut keys = vec![SEA_KEY; n_x * n_y];
    let mut data = vec![0u8; n_x * n_y * 4];
    for j in 0..n_y {
        let y = y_top - (j as f32 + 0.5) * POLITICAL_TEXEL;
        for i in 0..n_x {
            let x = x0 + (i as f32 + 0.5) * texel_w;
            let (q, r) = geometry::world_to_hex(Vec2::new(x, y));
            let (wq, wr) = borders::wrap_axial(q, r, map_width);
            if let Some(&ti) = index.get(&(wq, wr)) {
                let idx = j * n_x + i;
                keys[idx] = tile_keys[ti];
                data[idx * 4..idx * 4 + 4].copy_from_slice(&tile_fills[ti]);
            }
        }
    }

    // Border pass: mark both sides of every key change involving land (a
    // land-land change is a country border, land-sea a coastline), wrapping
    // the horizontal neighbor across the seam.
    let mut border = vec![false; n_x * n_y];
    for j in 0..n_y {
        for i in 0..n_x {
            let idx = j * n_x + i;
            let right = j * n_x + (i + 1) % n_x;
            if keys[idx] != keys[right] {
                border[idx] = true;
                border[right] = true;
            }
            if j + 1 < n_y {
                let down = idx + n_x;
                if keys[idx] != keys[down] {
                    border[idx] = true;
                    border[down] = true;
                }
            }
        }
    }
    for (idx, marked) in border.iter().enumerate() {
        if !marked {
            continue;
        }
        if keys[idx] == SEA_KEY {
            // Coast ink over the water texture below: premarked color at the
            // stroke alpha, blended by the GPU.
            data[idx * 4] = BORDER_RGB[0];
            data[idx * 4 + 1] = BORDER_RGB[1];
            data[idx * 4 + 2] = BORDER_RGB[2];
            data[idx * 4 + 3] = (BORDER_ALPHA * 255.0) as u8;
        } else {
            for c in 0..3 {
                let base = f32::from(data[idx * 4 + c]);
                data[idx * 4 + c] =
                    (base + (f32::from(BORDER_RGB[c]) - base) * BORDER_ALPHA).round() as u8;
            }
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: n_x as u32,
            height: n_y as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    // Pixel art: scale up crisp, never blur.
    image.sampler = ImageSampler::nearest();

    Some(PoliticalRaster {
        image,
        min: Vec2::new(x0, y_top - n_y as f32 * POLITICAL_TEXEL),
        max: Vec2::new(x0 + period, y_top),
    })
}

/// Quad mesh mapping the raster once over its world rect: UV (0,0) at the
/// top-left `(min.x, max.y)` (image row 0 is the top). Spawn with a
/// local-UV material (no world-UV rewrite, no wrap phase shift).
pub fn raster_quad_mesh(min: Vec2, max: Vec2) -> Mesh {
    let positions = vec![
        [min.x, max.y, 0.0],
        [max.x, max.y, 0.0],
        [max.x, min.y, 0.0],
        [min.x, min.y, 0.0],
    ];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// `Color` → sRGB bytes (alpha forced opaque — land fills).
fn rgba(color: Color) -> [u8; 4] {
    let c = color.to_srgba();
    [
        (c.red * 255.0).round() as u8,
        (c.green * 255.0).round() as u8,
        (c.blue * 255.0).round() as u8,
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    fn tile(q: i32, r: i32, terrain: &str, owner: &str, owner_color: &str) -> MapTile {
        MapTile {
            q,
            r,
            map_width: 8,
            map_height: 8,
            terrain: terrain.to_string(),
            owner: owner.to_string(),
            owner_color: owner_color.to_string(),
            nation_id: 0,
            province: "P".to_string(),
            province_id: None,
            is_capital: false,
            is_country_capital: false,
            is_minor: false,
            is_incorporated_minor: false,
            incorporated_nation_id: None,
            is_anarchic: false,
            is_prospected: true,
            resource: None,
            resource_hidden: false,
            improvement_level: 0,
            max_improvement_level: 0,
            rail_links: Vec::new(),
            has_depot: false,
            has_port: false,
            has_fort: false,
            has_river: false,
            fort_level: 0,
            port_blockaded: false,
            army_unit_count: 0,
            army_firepower: 0.0,
            army_composition: None,
            naval_ship_count: 0,
            naval_firepower: 0,
            civilian_on_tile: None,
            visible: true,
            visual_group: None,
        }
    }

    fn texel_at(raster: &PoliticalRaster, world: Vec2) -> [u8; 4] {
        let size = raster.image.size();
        let (n_x, n_y) = (size.x as usize, size.y as usize);
        let texel_w = (raster.max.x - raster.min.x) / n_x as f32;
        let i = (((world.x - raster.min.x) / texel_w) as usize).min(n_x - 1);
        let j = (((raster.max.y - world.y) / POLITICAL_TEXEL) as usize).min(n_y - 1);
        let idx = j * n_x + i;
        let data = raster.image.data.as_ref().expect("raster pixel data");
        [
            data[idx * 4],
            data[idx * 4 + 1],
            data[idx * 4 + 2],
            data[idx * 4 + 3],
        ]
    }

    #[test]
    fn land_texels_take_political_fill_and_sea_stays_transparent() {
        let tiles = vec![
            tile(0, 0, "Grassland", "Alpha", "Red"),
            tile(1, 0, "Sea", "", ""),
        ];
        let raster = build_raster(&tiles, MapMode::Political, &HashMap::new()).unwrap();

        let land = texel_at(&raster, geometry::hex_to_world(0, 0));
        let expected = rgba(tile_fill_color(
            &tiles[0],
            MapMode::Political,
            &HashMap::new(),
        ));
        assert_eq!(land, expected, "land center texel carries the nation fill");
        assert_eq!(
            texel_at(&raster, geometry::hex_to_world(3, 3))[3],
            0,
            "open sea texels are transparent"
        );
    }

    #[test]
    fn country_boundary_and_coast_are_inked() {
        let tiles = vec![
            tile(0, 0, "Grassland", "Alpha", "Red"),
            tile(1, 0, "Grassland", "Beta", "Blue"),
        ];
        let raster = build_raster(&tiles, MapMode::Political, &HashMap::new()).unwrap();

        // Midpoint of the shared edge: country border ink (dark, opaque).
        let mid = (geometry::hex_to_world(0, 0) + geometry::hex_to_world(1, 0)) / 2.0;
        let border = texel_at(&raster, mid);
        assert_eq!(border[3], 255);
        assert!(
            border[0] < 40 && border[1] < 40 && border[2] < 40,
            "country boundary texel must be inked dark, got {border:?}"
        );
        // Sea texels hugging the coast carry translucent coast ink — and
        // translucent texels are only ever coast ink.
        let data = raster.image.data.as_ref().unwrap();
        let translucent: Vec<[u8; 4]> = data
            .chunks_exact(4)
            .filter(|px| px[3] > 0 && px[3] < 255)
            .map(|px| [px[0], px[1], px[2], px[3]])
            .collect();
        assert!(
            !translucent.is_empty(),
            "coastline must ink sea-side texels"
        );
        for px in &translucent {
            assert_eq!(px[..3], BORDER_RGB, "translucent texels are coast ink");
        }
    }

    #[test]
    fn raster_wraps_across_the_horizontal_seam() {
        // One full ring of same-owner land in row 0 across a 4-wide map:
        // the seam column must sample land, not fall off the map.
        let mut tiles: Vec<MapTile> = (0..4)
            .map(|q| tile(q, 0, "Grassland", "Alpha", "Red"))
            .collect();
        for t in &mut tiles {
            t.map_width = 4;
        }
        let raster = build_raster(&tiles, MapMode::Political, &HashMap::new()).unwrap();
        let size = raster.image.size();
        let n_x = size.x as usize;
        let row = (size.y / 2) as usize;
        let data = raster.image.data.as_ref().unwrap();
        let texel = |i: usize| {
            let idx = row * n_x + i;
            [
                data[idx * 4],
                data[idx * 4 + 1],
                data[idx * 4 + 2],
                data[idx * 4 + 3],
            ]
        };
        // Every texel of the middle row is opaque land of the same color:
        // the wrap lookup filled the seam columns and no country border was
        // inked inside a single-owner ring.
        let first = texel(0);
        assert_eq!(first[3], 255);
        for i in 1..n_x {
            assert_eq!(texel(i), first, "texel column {i} differs at the seam");
        }
    }

    #[test]
    fn overlay_mode_recolors_from_the_fill_map() {
        let tiles = vec![tile(0, 0, "Grassland", "Alpha", "Red")];
        let mut fill_map = HashMap::new();
        fill_map.insert("Alpha".to_string(), theme::OVERLAY_SELF);
        let raster = build_raster(&tiles, MapMode::Diplomatic, &fill_map).unwrap();
        assert_eq!(
            texel_at(&raster, geometry::hex_to_world(0, 0)),
            rgba(theme::OVERLAY_SELF)
        );
    }
}
