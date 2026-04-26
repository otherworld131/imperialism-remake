//! Glue between `crates/domain::GameState` and `crates/flavor`.
//!
//! Kept out of the domain crate on purpose: domain has no dependency on
//! `flavor`. Call this once right after constructing a `GameState`. It
//! populates the display-only strings (`adjective`, `demonym_plural`,
//! `government_title`, `flag_svg`, …) on every `Nation` and overwrites
//! `name` with the flavor-generated short form. Nothing in the
//! turn-resolution pipeline reads these fields — they're strictly UI.

use domain::game_state::GameState;
use domain::types::{NationId, NationType};
use flavor::{FlagRules, Rng, generate_city_names, generate_for_seed, load_default_mixes};
use std::collections::HashMap;

/// DJB2-style hash of a string — matches the seed derivation used by
/// `crates/domain/src/map/generator.rs`, so flavor seeding stays consistent
/// with other map-key-derived randomness.
fn djb2(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

/// Derive a per-nation seed so regenerating flavor for a single nation is
/// deterministic and independent of the iteration order.
fn nation_seed(base_seed: u64, nation_index: u32) -> u64 {
    // Splitmix64 step on `base_seed ^ index` — cheap and spreads the bits.
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
/// `game.map_key`, keeping replays stable on the same map.
pub fn apply_flavor(game: &mut GameState, flavor_key: &str) {
    let key = if flavor_key.is_empty() {
        game.map_key.as_str()
    } else {
        flavor_key
    };
    let base_seed = djb2(key);
    let rules = FlagRules::default();
    let (gp_mix, mn_mix) = load_default_mixes();

    for nation in game.nations.iter_mut() {
        // Skip rehydration: if flavor fields were already populated (e.g.
        // loaded from a save), leave them alone.
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
/// the names are deterministic per (flavor_key, owner) and unique within
/// an owner's own provinces.
fn apply_province_names(game: &mut GameState, base_seed: u64) {
    let mut by_owner: HashMap<NationId, Vec<usize>> = HashMap::new();
    for (i, prov) in game.provinces.iter().enumerate() {
        by_owner.entry(prov.owner).or_default().push(i);
    }
    for (owner_id, mut indices) in by_owner {
        indices.sort_by_key(|i| game.provinces[*i].id.0);
        let owner_seed = nation_seed(base_seed, owner_id.0);
        let mut rng = Rng::from_seed(owner_seed);
        let names = generate_city_names(&mut rng, indices.len());
        for (k, prov_idx) in indices.iter().enumerate() {
            if let Some(name) = names.get(k) {
                game.provinces[*prov_idx].name = name.clone();
            }
        }
    }
}
