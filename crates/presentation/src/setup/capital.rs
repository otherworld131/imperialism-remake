//! Capital-placement logic ported from the web frontend
//! (`GameSetup.logic.ts`): valid tiles, the 7-hex opening-yield preview
//! (surface resources, river/coastal fish, food supply, worker support) and
//! the ranked top-5 suggested placements.

use std::collections::HashMap;

use crate::game::vm::MapTile;

/// Opening yield per worked deposit at game start (web constants).
const SURFACE_RESOURCE_YIELD: u32 = 2;
const HEAVY_DEPOSIT_YIELD: u32 = 2;
const PRECIOUS_DEPOSIT_YIELD: u32 = 1;

/// Display order for the yields panel.
const RESOURCE_ORDER: [&str; 13] = [
    "Grain",
    "Fruit",
    "Livestock",
    "Fish",
    "Timber",
    "Cotton",
    "Wool",
    "Horses",
    "Coal",
    "Iron",
    "Gold",
    "Gems",
    "Oil",
];

#[derive(Debug, Clone, PartialEq)]
pub struct CapitalPreview {
    pub q: i32,
    pub r: i32,
    /// Workers supportable by the opening food supply
    /// (`frontend_api::setup::max_workers_supportable`).
    pub support: u32,
    /// `(resource, opening amount)` in display order.
    pub resources: Vec<(String, u32)>,
    pub collected_tiles: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub preview: CapitalPreview,
    pub province_name: String,
    /// Compass direction from the site's own province centroid ("NW", or
    /// "Center" when the site sits near it) — distinguishes rows that
    /// share a province name.
    pub direction: &'static str,
    /// Any adjacent sea hex (coastal capitals fish up to 3 extra).
    pub coastal: bool,
}

/// A capital may sit on any own land hex that isn't Sea or Mountain.
pub fn is_valid_capital_tile(tile: &MapTile, nation_id: i64) -> bool {
    tile.nation_id == nation_id && tile.terrain != "Sea" && tile.terrain != "Mountain"
}

fn resource_yield_at_start(resource: &str) -> u32 {
    match resource {
        "Coal" | "Iron" | "Oil" => HEAVY_DEPOSIT_YIELD,
        "Gold" | "Gems" => PRECIOUS_DEPOSIT_YIELD,
        _ => SURFACE_RESOURCE_YIELD,
    }
}

/// Evaluate the opening capital yield at `center` for `nation_id`:
/// the capital works its own hex plus the six neighbors (own, non-Sea),
/// collecting surface resources, +1 grain on bare grassland, +1 fish per
/// river tile, and up to 3 fish from adjacent sea hexes. Worker support is
/// resolved from the food supply via the domain's labor table.
pub fn evaluate_capital_site(
    center: &MapTile,
    by_coord: &HashMap<(i32, i32), usize>,
    tiles: &[MapTile],
    nation_id: i64,
) -> Option<CapitalPreview> {
    if !is_valid_capital_tile(center, nation_id) {
        return None;
    }
    let coords = [
        (center.q, center.r),
        (center.q + 1, center.r),
        (center.q + 1, center.r - 1),
        (center.q, center.r - 1),
        (center.q - 1, center.r),
        (center.q - 1, center.r + 1),
        (center.q, center.r + 1),
    ];
    let tile_at = |(q, r): (i32, i32)| by_coord.get(&(q, r)).map(|&i| &tiles[i]);

    let mut resources: HashMap<String, u32> = HashMap::new();
    let mut collected_tiles = 0u32;
    let (mut grain, mut fruit, mut meat) = (0u32, 0u32, 0u32);

    for coord in coords {
        let Some(tile) = tile_at(coord) else {
            continue;
        };
        if tile.nation_id != nation_id || tile.terrain == "Sea" {
            continue;
        }
        collected_tiles += 1;
        if let Some(resource) = tile.resource.as_deref() {
            let qty = resource_yield_at_start(resource);
            *resources.entry(resource.to_string()).or_insert(0) += qty;
            match resource {
                "Grain" => grain += qty,
                "Fruit" => fruit += qty,
                "Livestock" => meat += qty,
                _ => {}
            }
        } else if tile.terrain == "Grassland" {
            *resources.entry("Grain".to_string()).or_insert(0) += 1;
            grain += 1;
        }
        if tile.has_river {
            *resources.entry("Fish".to_string()).or_insert(0) += 1;
            meat += 1;
        }
    }

    // Up to three fish from adjacent sea hexes (coastal capital bonus).
    let coastal_fish = coords[1..]
        .iter()
        .filter(|&&coord| tile_at(coord).is_some_and(|t| t.terrain == "Sea"))
        .count()
        .min(3) as u32;
    if coastal_fish > 0 {
        *resources.entry("Fish".to_string()).or_insert(0) += coastal_fish;
        meat += coastal_fish;
    }

    let support = frontend_api::setup::max_workers_supportable(grain, fruit, meat);
    let ordered: Vec<(String, u32)> = RESOURCE_ORDER
        .iter()
        .filter_map(|&name| {
            resources
                .get(name)
                .filter(|&&amount| amount > 0)
                .map(|&amount| (name.to_string(), amount))
        })
        .collect();

    Some(CapitalPreview {
        q: center.q,
        r: center.r,
        support,
        resources: ordered,
        collected_tiles,
    })
}

/// 8-way compass direction of `(q, r)` seen from its own province's
/// centroid (plan: "compass direction from province center"); "Center"
/// within ~1.5 hexes of it.
fn compass_from_province_center(
    tiles: &[MapTile],
    nation_id: i64,
    province: &str,
    q: i32,
    r: i32,
) -> &'static str {
    use crate::map::geometry::{HEX_SIZE, hex_to_world};
    let mut sum = bevy::prelude::Vec2::ZERO;
    let mut count = 0u32;
    for tile in tiles {
        if tile.nation_id == nation_id && !tile.is_sea() && tile.province == province {
            sum += hex_to_world(tile.q, tile.r);
            count += 1;
        }
    }
    if count == 0 {
        return "Center";
    }
    let delta = hex_to_world(q, r) - sum / count as f32;
    if delta.length() < HEX_SIZE * 1.5 {
        return "Center";
    }
    // atan2 angle → one of 8 sectors, N at the top.
    let sector = ((delta.y.atan2(delta.x).to_degrees() + 360.0 + 22.5) / 45.0) as u32 % 8;
    ["E", "NE", "N", "NW", "W", "SW", "S", "SE"][sector as usize]
}

/// Any of the six neighbors is sea.
fn is_coastal(by_coord: &HashMap<(i32, i32), usize>, tiles: &[MapTile], q: i32, r: i32) -> bool {
    [
        (q + 1, r),
        (q + 1, r - 1),
        (q, r - 1),
        (q - 1, r),
        (q - 1, r + 1),
        (q, r + 1),
    ]
    .iter()
    .any(|coord| {
        by_coord
            .get(coord)
            .is_some_and(|&i| tiles[i].terrain == "Sea")
    })
}

/// Rank every valid tile and return the top five placements
/// (support → total yield → province name → coords; web parity).
pub fn suggest_capitals(
    tiles: &[MapTile],
    by_coord: &HashMap<(i32, i32), usize>,
    nation_id: i64,
) -> Vec<Suggestion> {
    let mut ranked: Vec<Suggestion> = tiles
        .iter()
        .filter(|tile| is_valid_capital_tile(tile, nation_id))
        .filter_map(|tile| {
            evaluate_capital_site(tile, by_coord, tiles, nation_id).map(|preview| Suggestion {
                preview,
                province_name: if tile.province.is_empty() {
                    "Unknown Province".to_string()
                } else {
                    tile.province.clone()
                },
                direction: compass_from_province_center(
                    tiles,
                    nation_id,
                    &tile.province,
                    tile.q,
                    tile.r,
                ),
                coastal: is_coastal(by_coord, tiles, tile.q, tile.r),
            })
        })
        .collect();
    ranked.sort_by(|a, b| {
        let yield_sum = |s: &Suggestion| s.preview.resources.iter().map(|(_, n)| *n).sum::<u32>();
        b.preview
            .support
            .cmp(&a.preview.support)
            .then(yield_sum(b).cmp(&yield_sum(a)))
            .then(a.province_name.cmp(&b.province_name))
            .then(a.preview.q.cmp(&b.preview.q))
            .then(a.preview.r.cmp(&b.preview.r))
    });
    ranked.truncate(5);
    ranked
}

/// Camera fit for the picked nation's territory: returns
/// `(center, ortho_scale)` so the whole nation fills the usable viewport
/// (the map area left of the sidebar), zoomed at most 4× in.
pub fn nation_view_fit(
    tiles: &[MapTile],
    nation_id: i64,
    usable: bevy::prelude::Vec2,
) -> Option<(bevy::prelude::Vec2, f32)> {
    use crate::map::geometry::{HEX_SIZE, SQRT_3, hex_to_world};
    let half_w = HEX_SIZE * SQRT_3 * 0.5;
    let mut min = bevy::prelude::Vec2::splat(f32::INFINITY);
    let mut max = bevy::prelude::Vec2::splat(f32::NEG_INFINITY);
    let mut any = false;
    for tile in tiles {
        if tile.nation_id != nation_id || tile.terrain == "Sea" {
            continue;
        }
        any = true;
        let p = hex_to_world(tile.q, tile.r);
        min = min.min(p - bevy::prelude::Vec2::new(half_w, HEX_SIZE));
        max = max.max(p + bevy::prelude::Vec2::new(half_w, HEX_SIZE));
    }
    if !any || usable.x <= 0.0 || usable.y <= 0.0 {
        return None;
    }
    let world = (max - min).max(bevy::prelude::Vec2::splat(HEX_SIZE * 2.0));
    let scale = (world.x / usable.x)
        .max(world.y / usable.y)
        .clamp(0.25, 2.8);
    Some(((min + max) * 0.5, scale))
}
