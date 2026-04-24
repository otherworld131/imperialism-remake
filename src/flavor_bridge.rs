//! Glue between `crates/domain::GameState` and `crates/flavor`.
//!
//! Kept out of the domain crate on purpose: domain has no dependency on
//! `flavor`. Call this once right after constructing a `GameState`. It
//! populates the display-only strings (`adjective`, `demonym_plural`,
//! `government_title`, `flag_svg`, …) on every `Nation`. Nothing in the
//! turn-resolution pipeline reads these fields — they're strictly UI.

use domain::game_state::GameState;
use domain::types::NationType;
use flavor::{FlagRules, GovernmentMix, Rng, generate_for_seed};

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

/// Apply procedural flavor (adjective, demonyms, government title, flag
/// SVG) to every nation in `game`. Existing gameplay `name` is preserved;
/// flavor augments — it never overwrites the core name used by the engine.
///
/// The seed is derived from `game.map_key` so replays on the same map
/// produce the same flavor.
pub fn apply_flavor(game: &mut GameState) {
    let base_seed = djb2(&game.map_key);
    let rules = FlagRules::default();
    let gp_mix = GovernmentMix::great_power_default();
    let mn_mix = GovernmentMix::minor_nation_default();

    for nation in game.nations.iter_mut() {
        // Skip rehydration: if flavor fields were already populated (e.g.
        // loaded from a save), leave them alone.
        if !nation.flag_svg.is_empty() {
            continue;
        }
        let is_gp = nation.nation_type == NationType::GreatPower;
        let mix = if is_gp { &gp_mix } else { &mn_mix };
        let s = nation_seed(base_seed, nation.id.0);
        let flavor = generate_for_seed(s, mix, &rules);
        nation.adjective = flavor.adjective;
        nation.demonym_singular = flavor.demonym_singular;
        nation.demonym_plural = flavor.demonym_plural;
        nation.government_title = flavor.government_title;
        nation.flag_svg = flavor.flag_svg;
    }
    // Touch the Rng import so the compiler doesn't warn when the helper
    // grows more entrypoints later.
    let _ = Rng::from_seed(base_seed);
}
