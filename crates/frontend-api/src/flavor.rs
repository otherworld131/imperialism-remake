//! Glue between domain `GameState` and the flavor crate.
//! This is the single shared implementation used by every frontend; the
//! former duplicates in the wasm-bridge crate and the top-level binary
//! import from here instead.

use domain::game_state::GameState;
use domain::types::{NationId, NationType};
use flavor::{FlagRules, Rng, generate_city_names, generate_for_seed, load_default_mixes};
use std::collections::HashMap;

fn djb2(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

fn nation_seed(base_seed: u64, nation_index: u32) -> u64 {
    let mut z = base_seed.wrapping_add((nation_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Apply procedural flavor (name, demonyms, government title, flag SVG) to
/// every nation in `game`. The flavor-generated `name` overwrites the
/// engine's static-pool name so the procedural country names actually appear
/// in-game. Existing flavor fields are skipped (so save reloads don't churn).
///
/// `flavor_key` seeds name + flag generation. An empty string falls back to
/// `game.world.map_key`, keeping replays stable on the same map.
pub fn apply_flavor(game: &mut GameState, flavor_key: &str) {
    let key = if flavor_key.is_empty() {
        game.world.map_key.as_str()
    } else {
        flavor_key
    };
    let base_seed = djb2(key);
    let rules = FlagRules::default();
    let (gp_mix, mn_mix) = load_default_mixes();

    for nation in game.world.nations.iter_mut() {
        if !nation.archives.flag_svg.is_empty() {
            continue;
        }
        let is_gp = nation.nation_type == NationType::GreatPower;
        let mix = if is_gp { &gp_mix } else { &mn_mix };
        let s = nation_seed(base_seed, nation.id.0);
        let flavor = generate_for_seed(s, mix, &rules);
        nation.name = flavor.name;
        nation.archives.adjective = flavor.adjective;
        nation.archives.demonym_singular = flavor.demonym_singular;
        nation.archives.demonym_plural = flavor.demonym_plural;
        nation.archives.government_title = flavor.government_title;
        nation.archives.flag_svg = flavor.flag_svg;
    }

    apply_province_names(game, base_seed);
}

/// Procedurally name every province. Each owner gets its own seeded RNG so
/// the names are deterministic per (flavor_key, owner) and the same flavor
/// seed always produces the same set. Names are deduplicated within an
/// owner's own provinces.
fn apply_province_names(game: &mut GameState, base_seed: u64) {
    let mut by_owner: HashMap<NationId, Vec<usize>> = HashMap::new();
    for (i, prov) in game.world.provinces.iter().enumerate() {
        by_owner.entry(prov.owner).or_default().push(i);
    }
    for (owner_id, mut indices) in by_owner {
        // Sort by province id so the name assignment is stable across
        // HashMap iteration orders.
        indices.sort_by_key(|i| game.world.provinces[*i].id.0);
        let owner_seed = nation_seed(base_seed, owner_id.0);
        let mut rng = Rng::from_seed(owner_seed);
        let names = generate_city_names(&mut rng, indices.len());
        for (k, prov_idx) in indices.iter().enumerate() {
            if let Some(name) = names.get(k) {
                game.world.provinces[*prov_idx].name = name.clone();
            }
        }
    }
}

/// Read-only query: every nation's identity card (name, color, type,
/// government title) plus its flag SVG, for frontends that rasterize flags
/// themselves (the native Bevy ledger / legend / battle screens). Returns
/// `[{nation_id, name, color, nation_type, government_title, flag_svg}]`;
/// nations without generated flavor carry empty strings. Additive — no wasm
/// export reads this.
pub fn get_nation_flags(game: &GameState) -> serde_json::Value {
    serde_json::Value::Array(
        game.world
            .nations
            .iter()
            .map(|n| {
                serde_json::json!({
                    "nation_id": n.id.0,
                    "name": n.name,
                    "color": format!("{:?}", n.color),
                    "nation_type": format!("{:?}", n.nation_type),
                    "government_title": n.archives.government_title,
                    "flag_svg": n.archives.flag_svg,
                })
            })
            .collect(),
    )
}

/// Wipe the flavor fields on every nation so a subsequent `apply_flavor`
/// regenerates them. Used by the "re-roll names" preview path.
pub fn clear_flavor(game: &mut GameState) {
    for nation in game.world.nations.iter_mut() {
        nation.archives.adjective.clear();
        nation.archives.demonym_singular.clear();
        nation.archives.demonym_plural.clear();
        nation.archives.government_title.clear();
        nation.archives.flag_svg.clear();
    }
}

/// Re-roll the flavor (names, flags, government titles) on an existing game
/// state, leaving everything else untouched. Used by GameSetup's
/// "Re-roll Names" button.
pub fn reroll_flavor(game: &mut GameState, flavor_key: &str) {
    clear_flavor(game);
    apply_flavor(game, flavor_key);
}
