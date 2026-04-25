//! Glue between domain `GameState` and the flavor crate for the web build.
//! Mirror of `src/flavor_bridge.rs` in the top-level binary — duplicated
//! here because the wasm-bridge crate can't import the binary's modules.
//! Kept tiny on purpose so divergence stays easy to spot.

use domain::game_state::GameState;
use domain::types::NationType;
use flavor::{FlagRules, generate_for_seed, load_default_mixes};

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
        if !nation.flag_svg.is_empty() {
            continue;
        }
        let is_gp = nation.nation_type == NationType::GreatPower;
        let mix = if is_gp { &gp_mix } else { &mn_mix };
        let s = nation_seed(base_seed, nation.id.0);
        let flavor = generate_for_seed(s, mix, &rules);
        nation.name = flavor.name;
        nation.adjective = flavor.adjective;
        nation.demonym_singular = flavor.demonym_singular;
        nation.demonym_plural = flavor.demonym_plural;
        nation.government_title = flavor.government_title;
        nation.flag_svg = flavor.flag_svg;
    }
}

/// Wipe the flavor fields on every nation so a subsequent `apply_flavor`
/// regenerates them. Used by the "re-roll names" preview path.
pub fn clear_flavor(game: &mut GameState) {
    for nation in game.nations.iter_mut() {
        nation.adjective.clear();
        nation.demonym_singular.clear();
        nation.demonym_plural.clear();
        nation.government_title.clear();
        nation.flag_svg.clear();
    }
}
